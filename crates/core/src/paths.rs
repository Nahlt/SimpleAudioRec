//! Cartella dei dati: accanto all'eseguibile se scrivibile (modalità portable),
//! altrimenti `%APPDATA%\SimpleAudioRec`.

use crate::APP_NAME;
use std::path::{Path, PathBuf};

fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".{}-write-test-{}", APP_NAME, std::process::id()));
    match std::fs::write(&probe, b"x") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn user_config_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join(APP_NAME);
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join(APP_NAME);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join(APP_NAME);
    }
    std::env::temp_dir().join(APP_NAME)
}

#[derive(Clone, Debug)]
pub struct DataPaths {
    pub dir: PathBuf,
}

impl DataPaths {
    pub fn detect() -> Self {
        if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf))
            && is_writable(&dir)
        {
            return Self { dir };
        }
        let dir = user_config_dir();
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.dir.join(format!("{APP_NAME}.toml"))
    }

    pub fn recovery_dir(&self) -> PathBuf {
        let d = self.dir.join("recovery");
        let _ = std::fs::create_dir_all(&d);
        d
    }

    pub fn mute_owner_file(&self) -> PathBuf {
        self.recovery_dir().join("mute-owner.toml")
    }
}
