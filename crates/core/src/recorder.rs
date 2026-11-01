//! Registrazione in corso: thread di cattura + thread di mixing che scrive il
//! file temporaneo.

use crate::audio::{AudioBackend, CaptureHandle, CaptureStatus};
use crate::fifo::SourceFifo;
use crate::mixer::MixerCore;
use crate::session::{Bookmark, SessionDir, SessionMeta};
use crate::settings::SourceSetting;
use crate::wav::WavWriter;
use crate::{SAMPLE_RATE, lock};
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Soglia di attività degli indicatori (−50 dBFS).
pub const ACTIVITY_THRESHOLD: f32 = 0.003_162;
/// Tenuta degli indicatori.
pub const ACTIVITY_HOLD_MS: u64 = 150;

/// Stato condiviso tra il mixer e l'interfaccia.
pub struct Shared {
    pub paused: AtomicBool,
    pub mic_muted: AtomicBool,
    stop: AtomicBool,
    pub recorded_frames: AtomicU64,
    mic_active_at: AtomicU64,
    out_active_at: AtomicU64,
    start: Instant,
    pub error: Mutex<Option<String>>,
}

impl Shared {
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64 + 1
    }

    fn active(&self, at: &AtomicU64) -> bool {
        let t = at.load(Ordering::Relaxed);
        t != 0 && self.now_ms().saturating_sub(t) <= ACTIVITY_HOLD_MS
    }

    pub fn mic_active(&self) -> bool {
        self.active(&self.mic_active_at)
    }

    pub fn out_active(&self) -> bool {
        self.active(&self.out_active_at)
    }

    pub fn frames(&self) -> u64 {
        self.recorded_frames.load(Ordering::Relaxed)
    }
}

pub struct Recorder {
    pub shared: Arc<Shared>,
    pub session: SessionDir,
    pub meta: SessionMeta,
    mixer: Option<JoinHandle<io::Result<u64>>>,
    mic_capture: Box<dyn CaptureHandle>,
    out_capture: Box<dyn CaptureHandle>,
}

/// Registrazione terminata, in attesa di salvataggio.
#[derive(Clone, Debug)]
pub struct Finished {
    pub session: SessionDir,
    pub meta: SessionMeta,
    pub frames: u64,
}

impl Recorder {
    pub fn start(
        backend: &dyn AudioBackend,
        input: &SourceSetting,
        output: &SourceSetting,
        session: SessionDir,
        meta: SessionMeta,
        mic_muted: bool,
    ) -> io::Result<Self> {
        session.save_meta(&meta)?;
        let mut wav = WavWriter::create(&session.audio_path(), 2, SAMPLE_RATE)?;
        let mic = SourceFifo::new();
        let out = SourceFifo::new();
        let shared = Arc::new(Shared {
            paused: AtomicBool::new(false),
            mic_muted: AtomicBool::new(mic_muted),
            stop: AtomicBool::new(false),
            recorded_frames: AtomicU64::new(0),
            mic_active_at: AtomicU64::new(0),
            out_active_at: AtomicU64::new(0),
            start: Instant::now(),
            error: Mutex::new(None),
        });
        let mic_capture = backend.start_input(input, mic.clone());
        let out_capture = backend.start_output(output, out.clone());
        let sh = shared.clone();
        let mixer = std::thread::Builder::new().name("mixer".into()).spawn(move || {
            let res = mixer_loop(&sh, mic, out, &mut wav);
            if let Err(e) = &res {
                *lock(&sh.error) = Some(e.to_string());
            }
            let fin = wav.finish(SAMPLE_RATE);
            res.and(fin)
        })?;
        Ok(Self { shared, session, meta, mixer: Some(mixer), mic_capture, out_capture })
    }

    pub fn is_paused(&self) -> bool {
        self.shared.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, p: bool) {
        self.shared.paused.store(p, Ordering::Relaxed);
    }

    pub fn set_mic_muted(&self, m: bool) {
        self.shared.mic_muted.store(m, Ordering::Relaxed);
    }

    /// Aggiunge un segnalibro all'istante corrente; restituisce il numero di segnalibri.
    pub fn add_bookmark(&mut self, label: impl FnOnce(usize) -> String) -> usize {
        let n = self.meta.bookmarks.len() + 1;
        self.meta.bookmarks.push(Bookmark { frame: self.shared.frames(), label: label(n) });
        if let Err(e) = self.session.save_meta(&self.meta) {
            log::warn!("salvataggio segnalibri: {e}");
        }
        n
    }

    pub fn statuses(&self) -> (CaptureStatus, CaptureStatus) {
        (self.mic_capture.status(), self.out_capture.status())
    }

    pub fn stop(mut self) -> io::Result<Finished> {
        self.shared.stop.store(true, Ordering::Relaxed);
        let res = self.mixer.take().map(|h| h.join());
        self.mic_capture.stop();
        self.out_capture.stop();
        let frames = match res {
            Some(Ok(r)) => r?,
            _ => self.session.frames(),
        };
        self.session.save_meta(&self.meta)?;
        Ok(Finished { session: self.session.clone(), meta: self.meta.clone(), frames })
    }
}

fn mixer_loop(sh: &Shared, mic: Arc<SourceFifo>, out: Arc<SourceFifo>, wav: &mut WavWriter) -> io::Result<()> {
    let mut core = MixerCore::new(mic, out);
    let mut inter = Vec::with_capacity(SAMPLE_RATE as usize);
    let mut timeline = 0u64;
    let mut last_sync = Instant::now();
    loop {
        let stopping = sh.stop.load(Ordering::Relaxed);
        if !stopping {
            std::thread::sleep(Duration::from_millis(10));
        }
        let target = (sh.start.elapsed().as_secs_f64() * SAMPLE_RATE as f64) as u64;
        let mut todo = target.saturating_sub(timeline);
        while todo > 0 {
            let n = todo.min(SAMPLE_RATE as u64 / 2) as usize;
            let peaks = core.tick(n);
            timeline += n as u64;
            todo -= n as u64;
            let muted = sh.mic_muted.load(Ordering::Relaxed);
            let now = sh.now_ms();
            if peaks.mic > ACTIVITY_THRESHOLD && !muted {
                sh.mic_active_at.store(now, Ordering::Relaxed);
            }
            if peaks.out > ACTIVITY_THRESHOLD {
                sh.out_active_at.store(now, Ordering::Relaxed);
            }
            if !sh.paused.load(Ordering::Relaxed) {
                core.interleave_i16(muted, &mut inter);
                wav.write(&inter)?;
                sh.recorded_frames.fetch_add(n as u64, Ordering::Relaxed);
            }
        }
        if stopping {
            return Ok(());
        }
        if last_sync.elapsed() >= Duration::from_secs(1) {
            wav.sync(SAMPLE_RATE)?;
            last_sync = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::NullBackend;
    use chrono::Local;

    #[test]
    fn record_pause_bookmark_stop() {
        let root = std::env::temp_dir().join(format!("sar-rec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let start = Local::now();
        let dir = SessionDir::create(&root, start).unwrap();
        let s = SourceSetting::default();
        let mut r = Recorder::start(&NullBackend, &s, &s, dir, SessionMeta::new(start), false).unwrap();
        std::thread::sleep(Duration::from_millis(400));
        r.set_paused(true);
        std::thread::sleep(Duration::from_millis(50));
        let paused_at = r.shared.frames();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(r.shared.frames(), paused_at);
        r.add_bookmark(|n| format!("B{n}"));
        r.set_paused(false);
        std::thread::sleep(Duration::from_millis(200));
        let f = r.stop().unwrap();
        let secs = f.frames as f64 / 48_000.0;
        assert!((0.5..0.75).contains(&secs), "{secs}");
        assert_eq!(f.meta.bookmarks[0].frame, paused_at);
        assert_eq!(f.session.frames(), f.frames);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
