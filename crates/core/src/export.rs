//! Salvataggio: dal WAV temporaneo agli MP3 finali (stereo separato e/o mono
//! mixato), con normalizzazione di loudness per canale, tag ID3, capitoli e
//! file di testo dei segnalibri.

use crate::limiter::Limiter;
use crate::mixer::to_i16;
use crate::naming::OutputPaths;
use crate::session::Bookmark;
use crate::wav::WavReader;
use crate::{APP_NAME, APP_VERSION, SAMPLE_RATE, format_hms};
use chrono::{DateTime, Datelike, FixedOffset, Timelike};
use id3::frame::{Chapter, TableOfContents, Timestamp};
use id3::{Frame, Tag, TagLike, Version};
use mp3lame_encoder::{Bitrate, Builder, Encoder, FlushNoGap, InterleavedPcm, Mode, MonoPcm, Quality};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Loudness obiettivo per canale (LUFS).
pub const TARGET_LUFS: f64 = -16.0;
pub const MIN_GAIN_DB: f64 = -10.0;
pub const MAX_GAIN_DB: f64 = 15.0;
/// Sotto questa loudness il canale è considerato silenzioso e non viene amplificato.
pub const SILENCE_LUFS: f64 = -70.0;
const LIMITER_DB: f32 = -1.0;
const CHUNK: usize = SAMPLE_RATE as usize;

#[derive(Clone, Debug)]
pub struct ExportSpec {
    pub wav: PathBuf,
    pub outputs: OutputPaths,
    pub bitrate_kbps: u32,
    pub normalize: bool,
    /// Titolo ID3 (es. "Registrazione 2026-11-01 14:30").
    pub title: String,
    pub started_at: Option<DateTime<FixedOffset>>,
    pub bookmarks: Vec<Bookmark>,
}

/// Bitrate del mono: metà di quello stereo, minimo 48 kbps.
pub fn mono_bitrate(stereo_kbps: u32) -> u32 {
    (stereo_kbps / 2).max(48)
}

fn bitrate(kbps: u32) -> Bitrate {
    match kbps {
        0..=48 => Bitrate::Kbps48,
        49..=64 => Bitrate::Kbps64,
        65..=80 => Bitrate::Kbps80,
        81..=96 => Bitrate::Kbps96,
        97..=128 => Bitrate::Kbps128,
        129..=160 => Bitrate::Kbps160,
        _ => Bitrate::Kbps192,
    }
}

/// Guadagno (dB) per portare la loudness `lufs` all'obiettivo.
pub fn gain_for(lufs: f64) -> f64 {
    if !lufs.is_finite() || lufs < SILENCE_LUFS {
        0.0
    } else {
        (TARGET_LUFS - lufs).clamp(MIN_GAIN_DB, MAX_GAIN_DB)
    }
}

struct Mp3Out {
    enc: Encoder,
    channels: u8,
    file: BufWriter<File>,
    buf: Vec<u8>,
    part: PathBuf,
}

fn part_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

impl Mp3Out {
    fn new(path: &Path, channels: u8, kbps: u32) -> Result<Self, String> {
        let mut b = Builder::new().ok_or("LAME non disponibile")?;
        b.set_num_channels(channels).map_err(|e| format!("{e:?}"))?;
        b.set_sample_rate(SAMPLE_RATE).map_err(|e| format!("{e:?}"))?;
        // Senza questa impostazione LAME sceglierebbe da sé una frequenza più bassa.
        b.set_output_sample_rate(NonZeroU32::new(SAMPLE_RATE)).map_err(|e| format!("{e:?}"))?;
        b.set_brate(bitrate(kbps)).map_err(|e| format!("{e:?}"))?;
        // Stereo puro (non joint): i due canali restano indipendenti.
        b.set_mode(if channels == 2 { Mode::Stereo } else { Mode::Mono }).map_err(|e| format!("{e:?}"))?;
        b.set_quality(Quality::VeryNice).map_err(|e| format!("{e:?}"))?;
        b.set_to_write_vbr_tag(false).map_err(|e| format!("{e:?}"))?;
        let enc = b.build().map_err(|e| format!("{e:?}"))?;
        let part = part_path(path);
        let file = File::create(&part).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self { enc, channels, file: BufWriter::new(file), buf: Vec::new(), part })
    }

    fn encode(&mut self, pcm: &[i16]) -> Result<(), String> {
        let frames = pcm.len() / self.channels as usize;
        self.buf.clear();
        self.buf.reserve(frames * 5 / 4 + 7200);
        let r = if self.channels == 2 {
            self.enc.encode_to_vec(InterleavedPcm(pcm), &mut self.buf)
        } else {
            self.enc.encode_to_vec(MonoPcm(pcm), &mut self.buf)
        };
        r.map_err(|e| format!("MP3: {e:?}"))?;
        self.file.write_all(&self.buf).map_err(|e| e.to_string())
    }

    fn finish(mut self) -> Result<PathBuf, String> {
        self.buf.clear();
        self.buf.reserve(7200);
        self.enc.flush_to_vec::<FlushNoGap>(&mut self.buf).map_err(|e| format!("MP3: {e:?}"))?;
        self.file.write_all(&self.buf).map_err(|e| e.to_string())?;
        self.file.flush().map_err(|e| e.to_string())?;
        Ok(self.part)
    }
}

fn measure(spec: &ExportSpec, progress: &AtomicU32, share: u32) -> Result<[f32; 2], String> {
    if !spec.normalize {
        return Ok([1.0, 1.0]);
    }
    let mut r = WavReader::open(&spec.wav).map_err(|e| e.to_string())?;
    let total = r.frames.max(1);
    let mut meters = [
        ebur128::EbuR128::new(1, SAMPLE_RATE, ebur128::Mode::I).map_err(|e| format!("{e:?}"))?,
        ebur128::EbuR128::new(1, SAMPLE_RATE, ebur128::Mode::I).map_err(|e| format!("{e:?}"))?,
    ];
    let mut pcm = vec![0i16; CHUNK * 2];
    let (mut l, mut rr) = (Vec::with_capacity(CHUNK), Vec::with_capacity(CHUNK));
    let mut done = 0u64;
    loop {
        let n = r.read(&mut pcm).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        l.clear();
        rr.clear();
        for f in pcm[..n * 2].chunks_exact(2) {
            l.push(f[0] as f32 / 32768.0);
            rr.push(f[1] as f32 / 32768.0);
        }
        meters[0].add_frames_f32(&l).map_err(|e| format!("{e:?}"))?;
        meters[1].add_frames_f32(&rr).map_err(|e| format!("{e:?}"))?;
        done += n as u64;
        progress.store((done * share as u64 / total) as u32, Ordering::Relaxed);
    }
    let g = |m: &ebur128::EbuR128| {
        let lufs = m.loudness_global().unwrap_or(f64::NEG_INFINITY);
        10f64.powf(gain_for(lufs) / 20.0) as f32
    };
    Ok([g(&meters[0]), g(&meters[1])])
}

struct Pipeline {
    lim: [Limiter; 3],
    stereo: Option<Mp3Out>,
    mono: Option<Mp3Out>,
    lo: Vec<f32>,
    ro: Vec<f32>,
    mo: Vec<f32>,
    pcm: Vec<i16>,
}

impl Pipeline {
    fn emit(&mut self) -> Result<(), String> {
        if let Some(s) = &mut self.stereo {
            self.pcm.clear();
            for (l, r) in self.lo.iter().zip(&self.ro) {
                self.pcm.push(to_i16(*l));
                self.pcm.push(to_i16(*r));
            }
            s.encode(&self.pcm)?;
        }
        if let Some(m) = &mut self.mono {
            self.pcm.clear();
            self.pcm.extend(self.mo.iter().map(|v| to_i16(*v)));
            m.encode(&self.pcm)?;
        }
        self.lo.clear();
        self.ro.clear();
        self.mo.clear();
        Ok(())
    }
}

fn new_limiter() -> Limiter {
    Limiter::new(SAMPLE_RATE, LIMITER_DB, 5.0, 100.0)
}

fn encode(spec: &ExportSpec, gains: [f32; 2], progress: &AtomicU32, from: u32) -> Result<Vec<PathBuf>, String> {
    let mut r = WavReader::open(&spec.wav).map_err(|e| e.to_string())?;
    let total = r.frames.max(1);
    let mut p = Pipeline {
        lim: [new_limiter(), new_limiter(), new_limiter()],
        stereo: spec.outputs.stereo.as_deref().map(|o| Mp3Out::new(o, 2, spec.bitrate_kbps)).transpose()?,
        mono: spec.outputs.mono.as_deref().map(|o| Mp3Out::new(o, 1, mono_bitrate(spec.bitrate_kbps))).transpose()?,
        lo: Vec::new(),
        ro: Vec::new(),
        mo: Vec::new(),
        pcm: Vec::new(),
    };
    let mut raw = vec![0i16; CHUNK * 2];
    let (mut l, mut rr, mut m) = (Vec::new(), Vec::new(), Vec::new());
    let mut done = 0u64;
    loop {
        let n = r.read(&mut raw).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        l.clear();
        rr.clear();
        m.clear();
        for f in raw[..n * 2].chunks_exact(2) {
            let a = f[0] as f32 / 32768.0 * gains[0];
            let b = f[1] as f32 / 32768.0 * gains[1];
            l.push(a);
            rr.push(b);
            m.push(a + b);
        }
        if p.stereo.is_some() {
            p.lim[0].process(&l, &mut p.lo);
            p.lim[1].process(&rr, &mut p.ro);
        }
        if p.mono.is_some() {
            p.lim[2].process(&m, &mut p.mo);
        }
        p.emit()?;
        done += n as u64;
        progress.store(from + (done * (1000 - from) as u64 / total) as u32, Ordering::Relaxed);
    }
    if p.stereo.is_some() {
        p.lim[0].flush(&mut p.lo);
        p.lim[1].flush(&mut p.ro);
    }
    if p.mono.is_some() {
        p.lim[2].flush(&mut p.mo);
    }
    p.emit()?;
    let mut parts = Vec::new();
    if let Some(s) = p.stereo.take() {
        parts.push(s.finish()?);
    }
    if let Some(m) = p.mono.take() {
        parts.push(m.finish()?);
    }
    Ok(parts)
}

fn tag(spec: &ExportSpec, frames: u64) -> Tag {
    let mut t = Tag::new();
    t.set_title(spec.title.clone());
    t.set_text("TSSE", format!("{APP_NAME} {APP_VERSION}"));
    let total_ms = (frames * 1000 / SAMPLE_RATE as u64) as u32;
    t.set_duration(total_ms);
    if let Some(s) = spec.started_at {
        t.set_date_recorded(Timestamp {
            year: s.year(),
            month: Some(s.month() as u8),
            day: Some(s.day() as u8),
            hour: Some(s.hour() as u8),
            minute: Some(s.minute() as u8),
            second: Some(s.second() as u8),
        });
    }
    if !spec.bookmarks.is_empty() {
        let mut ids = Vec::new();
        for (i, b) in spec.bookmarks.iter().enumerate() {
            let id = format!("chp{i}");
            let start = (b.frame * 1000 / SAMPLE_RATE as u64) as u32;
            let end = spec
                .bookmarks
                .get(i + 1)
                .map(|n| (n.frame * 1000 / SAMPLE_RATE as u64) as u32)
                .unwrap_or(total_ms)
                .max(start);
            t.add_frame(Chapter {
                element_id: id.clone(),
                start_time: start,
                end_time: end,
                start_offset: u32::MAX,
                end_offset: u32::MAX,
                frames: vec![Frame::text("TIT2", b.label.clone())],
            });
            ids.push(id);
        }
        t.add_frame(TableOfContents {
            element_id: "toc".into(),
            top_level: true,
            ordered: true,
            elements: ids,
            frames: Vec::new(),
        });
    }
    t
}

/// Testo del file dei segnalibri: una riga `hh:mm:ss  descrizione` per segnalibro.
pub fn bookmarks_text(bookmarks: &[Bookmark]) -> String {
    bookmarks.iter().map(|b| format!("{}  {}\r\n", format_hms(b.frame), b.label)).collect()
}

/// Esegue il salvataggio. `progress` va da 0 a 1000. Restituisce i file creati.
pub fn run(spec: &ExportSpec, progress: &AtomicU32) -> Result<Vec<PathBuf>, String> {
    progress.store(0, Ordering::Relaxed);
    let frames = WavReader::frames_in(&spec.wav);
    let measure_share = if spec.normalize { 250 } else { 0 };
    let result = (|| {
        let gains = measure(spec, progress, measure_share)?;
        let parts = encode(spec, gains, progress, measure_share)?;
        let finals: Vec<&PathBuf> = [&spec.outputs.stereo, &spec.outputs.mono].into_iter().flatten().collect();
        let t = tag(spec, frames);
        let mut created = Vec::new();
        for (part, fin) in parts.iter().zip(finals) {
            t.write_to_path(part, Version::Id3v24).map_err(|e| format!("ID3: {e}"))?;
            std::fs::rename(part, fin).map_err(|e| format!("{}: {e}", fin.display()))?;
            created.push(fin.clone());
        }
        if let Some(bm) = &spec.outputs.bookmarks {
            std::fs::write(bm, bookmarks_text(&spec.bookmarks)).map_err(|e| format!("{}: {e}", bm.display()))?;
            created.push(bm.clone());
        }
        Ok(created)
    })();
    if result.is_err() {
        for p in [&spec.outputs.stereo, &spec.outputs.mono].into_iter().flatten() {
            let _ = std::fs::remove_file(part_path(p));
        }
    }
    progress.store(1000, Ordering::Relaxed);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::WavWriter;
    use std::f32::consts::PI;

    #[test]
    fn gains() {
        assert_eq!(gain_for(-26.0), 10.0);
        assert_eq!(gain_for(-40.0), 15.0);
        assert_eq!(gain_for(-3.0), -10.0);
        assert_eq!(gain_for(f64::NEG_INFINITY), 0.0);
        assert_eq!(gain_for(-80.0), 0.0);
        assert_eq!(mono_bitrate(64), 48);
        assert_eq!(mono_bitrate(192), 96);
    }

    #[test]
    fn export_stereo_and_mono_with_bookmarks() {
        let dir = std::env::temp_dir().join(format!("sar-exp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("audio.wav");
        let mut w = WavWriter::create(&wav, 2, SAMPLE_RATE).unwrap();
        // 3 s: tono a 440 Hz sul canale sinistro (−30 dBFS), silenzio a destra
        let mut pcm = Vec::new();
        for i in 0..SAMPLE_RATE as usize * 3 {
            let v = (2.0 * PI * 440.0 * i as f32 / SAMPLE_RATE as f32).sin() * 0.0316;
            pcm.push(to_i16(v));
            pcm.push(0);
        }
        w.write(&pcm).unwrap();
        w.finish(SAMPLE_RATE).unwrap();
        let outputs = crate::naming::outputs_from_chosen(
            &dir.join("x_stereo.mp3"),
            crate::settings::Format::Stereo,
            true,
            "_segnalibri",
            true,
        );
        let spec = ExportSpec {
            wav,
            outputs: outputs.clone(),
            bitrate_kbps: 96,
            normalize: true,
            title: "Registrazione di prova".into(),
            started_at: DateTime::parse_from_rfc3339("2026-11-01T14:30:00+01:00").ok(),
            bookmarks: vec![
                Bookmark { frame: 48_000, label: "Inizio".into() },
                Bookmark { frame: 96_000, label: "Decisione".into() },
            ],
        };
        let progress = AtomicU32::new(0);
        let created = run(&spec, &progress).unwrap();
        assert_eq!(created.len(), 3);
        assert_eq!(progress.load(Ordering::Relaxed), 1000);
        let stereo = std::fs::metadata(outputs.stereo.as_ref().unwrap()).unwrap().len();
        // 3 s a 96 kbps ≈ 36 kB (+ tag)
        assert!((30_000..45_000).contains(&stereo), "{stereo}");
        let t = Tag::read_from_path(outputs.stereo.as_ref().unwrap()).unwrap();
        assert_eq!(t.title(), Some("Registrazione di prova"));
        assert_eq!(t.chapters().count(), 2);
        let txt = std::fs::read_to_string(outputs.bookmarks.as_ref().unwrap()).unwrap();
        assert_eq!(txt, "00:00:01  Inizio\r\n00:00:02  Decisione\r\n");
        assert!(!dir.join("x_stereo.mp3.part").exists());
        // Intestazione del primo frame MP3: 48 kHz, stereo puro (non joint).
        for (path, mode) in [(outputs.stereo.as_ref().unwrap(), 0u32), (outputs.mono.as_ref().unwrap(), 3u32)] {
            let d = std::fs::read(path).unwrap();
            let tag_len = 10 + ((d[6] as usize) << 21 | (d[7] as usize) << 14 | (d[8] as usize) << 7 | d[9] as usize);
            let h = u32::from_be_bytes([d[tag_len], d[tag_len + 1], d[tag_len + 2], d[tag_len + 3]]);
            assert_eq!(h >> 21, 0x7ff, "sync");
            assert_eq!((h >> 10) & 3, 1, "48 kHz");
            assert_eq!((h >> 6) & 3, mode, "modalità canali");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
