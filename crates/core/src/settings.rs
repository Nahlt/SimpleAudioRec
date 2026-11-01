//! Impostazioni persistenti (`SimpleAudioRec.toml`).

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

/// Bitrate MP3 selezionabili (kbps).
pub const BITRATES: [u32; 5] = [64, 96, 128, 160, 192];

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum SourceMode {
    /// Dispositivo predefinito per comunicazioni (default).
    #[default]
    Communications,
    /// Dispositivo predefinito di sistema.
    System,
    /// Dispositivo scelto esplicitamente (`id`).
    Device,
    /// Solo l'audio di un'applicazione (solo output; `name` = eseguibile).
    App,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct SourceSetting {
    pub mode: SourceMode,
    /// Identificativo del dispositivo (modalità `Device`).
    pub id: String,
    /// Nome mostrato (dispositivo) o nome dell'eseguibile (modalità `App`).
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Stereo,
    Mono,
}

impl Format {
    pub fn other(self) -> Self {
        match self {
            Format::Stereo => Format::Mono,
            Format::Mono => Format::Stereo,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Auto,
    It,
    En,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(default)]
pub struct Hotkeys {
    pub start: String,
    pub pause: String,
    pub stop: String,
    pub mute: String,
    pub bookmark: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            start: "Ctrl+Alt+R".into(),
            pause: "Ctrl+Alt+P".into(),
            stop: "Ctrl+Alt+S".into(),
            mute: "Ctrl+Alt+M".into(),
            bookmark: "Ctrl+Alt+B".into(),
        }
    }
}

/// Azioni associabili a un hotkey globale.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Action {
    Start,
    Pause,
    Stop,
    Mute,
    Bookmark,
}

impl Action {
    pub const ALL: [Action; 5] = [Action::Start, Action::Pause, Action::Stop, Action::Mute, Action::Bookmark];
}

impl Hotkeys {
    pub fn get(&self, a: Action) -> &str {
        match a {
            Action::Start => &self.start,
            Action::Pause => &self.pause,
            Action::Stop => &self.stop,
            Action::Mute => &self.mute,
            Action::Bookmark => &self.bookmark,
        }
    }

    pub fn set(&mut self, a: Action, v: String) {
        match a {
            Action::Start => self.start = v,
            Action::Pause => self.pause = v,
            Action::Stop => self.stop = v,
            Action::Mute => self.mute = v,
            Action::Bookmark => self.bookmark = v,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Settings {
    pub language: Lang,
    pub input: SourceSetting,
    pub output: SourceSetting,
    pub format: Format,
    pub both_formats: bool,
    pub bitrate_kbps: u32,
    pub normalize: bool,
    pub autosave: bool,
    pub last_dir: Option<PathBuf>,
    pub window_pos: Option<[f32; 2]>,
    pub hotkeys: Hotkeys,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Lang::Auto,
            input: SourceSetting::default(),
            output: SourceSetting::default(),
            format: Format::Stereo,
            both_formats: false,
            bitrate_kbps: 96,
            normalize: true,
            autosave: false,
            last_dir: None,
            window_pos: None,
            hotkeys: Hotkeys::default(),
        }
    }
}

impl Settings {
    /// Carica le impostazioni; se il file non esiste usa i default, se è
    /// illeggibile usa i default e restituisce un messaggio d'errore.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(s) => match toml::from_str::<Settings>(&s) {
                Ok(mut st) => {
                    st.sanitize();
                    (st, None)
                }
                Err(e) => (Self::default(), Some(e.to_string())),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => (Self::default(), None),
            Err(e) => (Self::default(), Some(e.to_string())),
        }
    }

    fn sanitize(&mut self) {
        if !BITRATES.contains(&self.bitrate_kbps) {
            self.bitrate_kbps = 96;
        }
        if self.input.mode == SourceMode::App {
            self.input.mode = SourceMode::Communications;
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let s = toml::to_string_pretty(self).map_err(io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, s)?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_partial_file() {
        let s = Settings {
            output: SourceSetting { mode: SourceMode::App, id: String::new(), name: "ms-teams.exe".into() },
            last_dir: Some(PathBuf::from("C:\\Users\\x\\Documents")),
            ..Settings::default()
        };
        let t = toml::to_string_pretty(&s).unwrap();
        let back: Settings = toml::from_str(&t).unwrap();
        assert_eq!(back, s);
        let partial: Settings = toml::from_str("bitrate_kbps = 128\n[hotkeys]\nmute = \"Ctrl+Shift+F9\"\n").unwrap();
        assert_eq!(partial.bitrate_kbps, 128);
        assert_eq!(partial.hotkeys.mute, "Ctrl+Shift+F9");
        assert_eq!(partial.hotkeys.start, "Ctrl+Alt+R");
        assert!(partial.normalize);
    }
}
