//! Nomi dei file prodotti.

use crate::settings::Format;
use chrono::{DateTime, TimeZone};
use std::path::{Path, PathBuf};

/// `AAAA-MM-GG_hh-mm` dall'istante di inizio.
pub fn base_name<Tz: TimeZone>(start: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    start.format("%Y-%m-%d_%H-%M").to_string()
}

pub fn suffix(f: Format) -> &'static str {
    match f {
        Format::Stereo => "_stereo",
        Format::Mono => "_mono",
    }
}

/// Nome file proposto per un formato: `AAAA-MM-GG_hh-mm_stereo.mp3`.
pub fn file_name(base: &str, f: Format) -> String {
    format!("{base}{}.mp3", suffix(f))
}

/// Percorsi di uscita di un salvataggio.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutputPaths {
    pub stereo: Option<PathBuf>,
    pub mono: Option<PathBuf>,
    pub bookmarks: Option<PathBuf>,
}

impl OutputPaths {
    pub fn all(&self) -> Vec<&PathBuf> {
        [&self.stereo, &self.mono, &self.bookmarks].into_iter().flatten().collect()
    }
}

/// Percorsi a partire dal file scelto nel dialogo "Salva con nome".
/// Il file scelto è quello del formato principale; l'eventuale secondo formato
/// viene creato accanto, sostituendo (o aggiungendo) il suffisso.
pub fn outputs_from_chosen(
    chosen: &Path,
    primary: Format,
    both: bool,
    bookmarks_suffix: &str,
    has_bookmarks: bool,
) -> OutputPaths {
    let mut chosen = chosen.to_path_buf();
    let is_mp3 = chosen.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mp3"));
    if !is_mp3 {
        let mut s = chosen.into_os_string();
        s.push(".mp3");
        chosen = PathBuf::from(s);
    }
    let dir = chosen.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = chosen.file_stem().and_then(|s| s.to_str()).unwrap_or("registrazione").to_string();
    let core = stem.strip_suffix(suffix(primary)).unwrap_or(&stem).to_string();
    let companion = dir.join(format!("{core}{}.mp3", suffix(primary.other())));
    let mut out = OutputPaths::default();
    match primary {
        Format::Stereo => {
            out.stereo = Some(chosen);
            out.mono = both.then_some(companion);
        }
        Format::Mono => {
            out.mono = Some(chosen);
            out.stereo = both.then_some(companion);
        }
    }
    if has_bookmarks {
        out.bookmarks = Some(dir.join(format!("{core}{bookmarks_suffix}.txt")));
    }
    out
}

/// Percorsi per il salvataggio automatico: aggiunge ` (2)`, ` (3)`… al nome base
/// finché nessuno dei file esiste già.
pub fn outputs_auto(
    dir: &Path,
    base: &str,
    primary: Format,
    both: bool,
    bookmarks_suffix: &str,
    has_bookmarks: bool,
) -> OutputPaths {
    let mut n = 1;
    loop {
        let b = if n == 1 { base.to_string() } else { format!("{base} ({n})") };
        let chosen = dir.join(file_name(&b, primary));
        let out = outputs_from_chosen(&chosen, primary, both, bookmarks_suffix, has_bookmarks);
        if out.all().iter().all(|p| !p.exists()) {
            return out;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn names() {
        let t = Local.with_ymd_and_hms(2026, 11, 1, 14, 30, 5).unwrap();
        assert_eq!(base_name(&t), "2026-11-01_14-30");
        assert_eq!(file_name("2026-11-01_14-30", Format::Mono), "2026-11-01_14-30_mono.mp3");
    }

    #[test]
    fn companion_paths() {
        let o = outputs_from_chosen(Path::new("/d/2026-11-01_14-30_stereo.mp3"), Format::Stereo, true, "_segnalibri", true);
        assert_eq!(o.mono.unwrap(), PathBuf::from("/d/2026-11-01_14-30_mono.mp3"));
        assert_eq!(o.bookmarks.unwrap(), PathBuf::from("/d/2026-11-01_14-30_segnalibri.txt"));
        let o = outputs_from_chosen(Path::new("/d/riunione"), Format::Mono, true, "_segnalibri", false);
        assert_eq!(o.mono.unwrap(), PathBuf::from("/d/riunione.mp3"));
        assert_eq!(o.stereo.unwrap(), PathBuf::from("/d/riunione_stereo.mp3"));
        assert!(o.bookmarks.is_none());
        let o = outputs_from_chosen(Path::new("/d/a_stereo.mp3"), Format::Stereo, false, "_x", false);
        assert!(o.mono.is_none());
    }

    #[test]
    fn auto_avoids_existing() {
        let dir = std::env::temp_dir().join(format!("sar-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b_mono.mp3"), b"x").unwrap();
        let o = outputs_auto(&dir, "b", Format::Stereo, true, "_s", false);
        assert_eq!(o.stereo.unwrap(), dir.join("b (2)_stereo.mp3"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
