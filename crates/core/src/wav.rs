//! File WAV temporaneo (PCM 16 bit) resistente ai crash.
//!
//! L'intestazione viene aggiornata periodicamente, ma la lettura non si fida dei
//! campi di lunghezza: il numero di frame è ricavato dalla dimensione del file.
//! Così un file troncato da un crash resta leggibile e non c'è limite di durata
//! (oltre i 4 GB i campi dell'intestazione sono saturati).

use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

const HEADER_LEN: u64 = 44;

pub struct WavWriter {
    file: BufWriter<File>,
    channels: u16,
    frames: u64,
}

fn header(channels: u16, rate: u32, data_bytes: u64) -> [u8; 44] {
    let block = channels as u32 * 2;
    let data = data_bytes.min(u32::MAX as u64 - 36) as u32;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * block).to_le_bytes());
    h[32..34].copy_from_slice(&(block as u16).to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data.to_le_bytes());
    h
}

impl WavWriter {
    pub fn create(path: &Path, channels: u16, rate: u32) -> io::Result<Self> {
        let mut f = OpenOptions::new().create(true).write(true).truncate(true).open(path)?;
        f.write_all(&header(channels, rate, 0))?;
        Ok(Self { file: BufWriter::with_capacity(1 << 18, f), channels, frames: 0 })
    }

    /// Accoda campioni interleaved.
    pub fn write(&mut self, samples: &[i16]) -> io::Result<()> {
        let mut bytes = Vec::with_capacity(samples.len() * 2);
        for s in samples {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        self.file.write_all(&bytes)?;
        self.frames += (samples.len() / self.channels as usize) as u64;
        Ok(())
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Scarica i dati sul file e aggiorna l'intestazione.
    pub fn sync(&mut self, rate: u32) -> io::Result<()> {
        self.file.flush()?;
        let data = self.frames * self.channels as u64 * 2;
        let f = self.file.get_mut();
        f.seek(SeekFrom::Start(0))?;
        f.write_all(&header(self.channels, rate, data))?;
        f.seek(SeekFrom::End(0))?;
        Ok(())
    }

    pub fn finish(mut self, rate: u32) -> io::Result<u64> {
        self.sync(rate)?;
        self.file.get_ref().sync_all()?;
        Ok(self.frames)
    }
}

pub struct WavReader {
    file: io::BufReader<File>,
    pub channels: u16,
    pub rate: u32,
    pub frames: u64,
    read_frames: u64,
}

impl WavReader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut f = File::open(path)?;
        let len = f.metadata()?.len();
        let mut h = [0u8; 44];
        f.read_exact(&mut h)?;
        if &h[0..4] != b"RIFF" || &h[8..12] != b"WAVE" || &h[36..40] != b"data" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "WAV non valido"));
        }
        let channels = u16::from_le_bytes([h[22], h[23]]).max(1);
        let rate = u32::from_le_bytes([h[24], h[25], h[26], h[27]]);
        let frames = len.saturating_sub(HEADER_LEN) / (channels as u64 * 2);
        Ok(Self { file: io::BufReader::with_capacity(1 << 18, f), channels, rate, frames, read_frames: 0 })
    }

    /// Legge fino a `buf.len() / channels` frame; restituisce i frame letti.
    pub fn read(&mut self, buf: &mut [i16]) -> io::Result<usize> {
        let ch = self.channels as usize;
        let want = ((buf.len() / ch) as u64).min(self.frames - self.read_frames) as usize;
        let mut bytes = vec![0u8; want * ch * 2];
        self.file.read_exact(&mut bytes)?;
        for (i, b) in bytes.chunks_exact(2).enumerate() {
            buf[i] = i16::from_le_bytes([b[0], b[1]]);
        }
        self.read_frames += want as u64;
        Ok(want)
    }

    /// Frame contenuti in un file WAV (0 se illeggibile).
    pub fn frames_in(path: &Path) -> u64 {
        Self::open(path).map(|r| r.frames).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_truncated_file() {
        let dir = std::env::temp_dir().join(format!("sar-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.wav");
        let mut w = WavWriter::create(&p, 2, 48_000).unwrap();
        let data: Vec<i16> = (0..2000).map(|i| i as i16).collect();
        w.write(&data).unwrap();
        w.sync(48_000).unwrap();
        // simula crash: il writer viene "perso" senza finish e il file ha dati oltre l'header
        w.write(&data).unwrap();
        drop(w);
        let mut r = WavReader::open(&p).unwrap();
        assert_eq!(r.frames, 2000);
        let mut buf = vec![0i16; 4000];
        assert_eq!(r.read(&mut buf).unwrap(), 2000);
        assert_eq!(&buf[..2000], &data[..]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
