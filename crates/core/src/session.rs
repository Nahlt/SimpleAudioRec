//! Sessione di registrazione su disco: audio temporaneo + metadati (inizio,
//! segnalibri), usati anche per il recupero dopo un crash.

use crate::wav::WavReader;
use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Bookmark {
    /// Posizione nella registrazione (frame a 48 kHz, pause escluse).
    pub frame: u64,
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct SessionMeta {
    /// Inizio della registrazione (RFC 3339, ora locale con offset).
    pub started_at: String,
    pub bookmarks: Vec<Bookmark>,
}

impl SessionMeta {
    pub fn new(start: DateTime<Local>) -> Self {
        Self { started_at: start.to_rfc3339(), bookmarks: Vec::new() }
    }

    pub fn start_time(&self) -> Option<DateTime<FixedOffset>> {
        DateTime::parse_from_rfc3339(&self.started_at).ok()
    }
}

#[derive(Clone, Debug)]
pub struct SessionDir {
    pub path: PathBuf,
}

const PREFIX: &str = "session-";

impl SessionDir {
    pub fn create(recovery_root: &Path, start: DateTime<Local>) -> io::Result<Self> {
        let mut path = recovery_root.join(format!("{PREFIX}{}", start.format("%Y%m%d-%H%M%S")));
        let mut n = 2;
        while path.exists() {
            path = recovery_root.join(format!("{PREFIX}{}-{n}", start.format("%Y%m%d-%H%M%S")));
            n += 1;
        }
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    pub fn audio_path(&self) -> PathBuf {
        self.path.join("audio.wav")
    }

    fn meta_path(&self) -> PathBuf {
        self.path.join("session.toml")
    }

    /// Salvataggio atomico dei metadati (file temporaneo + rinomina).
    pub fn save_meta(&self, meta: &SessionMeta) -> io::Result<()> {
        let s = toml::to_string_pretty(meta).map_err(io::Error::other)?;
        let tmp = self.path.join("session.toml.tmp");
        std::fs::write(&tmp, s)?;
        std::fs::rename(&tmp, self.meta_path())
    }

    pub fn load_meta(&self) -> SessionMeta {
        let parsed = std::fs::read_to_string(self.meta_path())
            .ok()
            .and_then(|s| toml::from_str::<SessionMeta>(&s).ok());
        let mut meta = parsed.unwrap_or_default();
        if meta.start_time().is_none() {
            meta.started_at = self.start_from_name().unwrap_or_else(Local::now).to_rfc3339();
        }
        meta
    }

    fn start_from_name(&self) -> Option<DateTime<Local>> {
        let name = self.path.file_name()?.to_str()?.strip_prefix(PREFIX)?;
        let stamp = name.get(..15)?;
        let naive = NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S").ok()?;
        Local.from_local_datetime(&naive).earliest()
    }

    pub fn frames(&self) -> u64 {
        WavReader::frames_in(&self.audio_path())
    }

    pub fn discard(&self) -> io::Result<()> {
        std::fs::remove_dir_all(&self.path)
    }
}

/// Registrazione presente su disco e non ancora salvata.
#[derive(Clone, Debug)]
pub struct Unsaved {
    pub dir: SessionDir,
    pub meta: SessionMeta,
    pub frames: u64,
}

/// Cerca le registrazioni non salvate; le sessioni vuote vengono eliminate.
pub fn find_unsaved(recovery_root: &Path) -> Vec<Unsaved> {
    let Ok(rd) = std::fs::read_dir(recovery_root) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(PREFIX)))
        .collect();
    dirs.sort();
    let mut out = Vec::new();
    for path in dirs {
        let dir = SessionDir { path };
        let frames = dir.frames();
        if frames == 0 {
            let _ = dir.discard();
            continue;
        }
        let meta = dir.load_meta();
        out.push(Unsaved { dir, meta, frames });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::WavWriter;

    #[test]
    fn find_and_discard() {
        let root = std::env::temp_dir().join(format!("sar-sess-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let start = Local::now();
        let empty = SessionDir::create(&root, start).unwrap();
        let full = SessionDir::create(&root, start).unwrap();
        assert_ne!(empty.path, full.path);
        let mut w = WavWriter::create(&full.audio_path(), 2, 48_000).unwrap();
        w.write(&vec![0i16; 9600]).unwrap();
        w.finish(48_000).unwrap();
        let mut meta = SessionMeta::new(start);
        meta.bookmarks.push(Bookmark { frame: 100, label: "x".into() });
        full.save_meta(&meta).unwrap();
        let found = find_unsaved(&root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].frames, 4800);
        assert_eq!(found[0].meta.bookmarks.len(), 1);
        assert!(!empty.path.exists());
        found[0].dir.discard().unwrap();
        assert!(find_unsaved(&root).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
