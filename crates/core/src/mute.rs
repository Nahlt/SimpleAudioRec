//! Mute di sistema su tutti i microfoni, con "possesso" persistente su disco
//! (RF-20 … RF-24).

use crate::audio::AudioBackend;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default)]
struct OwnerFile {
    devices: Vec<String>,
}

pub struct MuteManager {
    owner_file: PathBuf,
    /// Mute attivato da SimpleAudioRec e non annullato dall'esterno.
    active: bool,
    /// Dispositivi silenziati da SimpleAudioRec.
    owned: BTreeSet<String>,
    /// Dispositivi già considerati quando il mute è stato attivato.
    known: BTreeSet<String>,
    /// Stato mostrato nella UI: mute del microfono registrato.
    displayed: bool,
}

impl MuteManager {
    /// Crea il gestore e ripristina un eventuale mute rimasto da un crash.
    /// Restituisce anche il numero di microfoni da cui il mute è stato rimosso.
    pub fn new(owner_file: PathBuf, backend: &dyn AudioBackend) -> (Self, usize) {
        let mut restored = 0;
        if let Some(prev) = std::fs::read_to_string(&owner_file)
            .ok()
            .and_then(|s| toml::from_str::<OwnerFile>(&s).ok())
        {
            let states = backend.capture_mute_states();
            for id in prev.devices {
                if states.iter().any(|(d, m)| *d == id && *m) && backend.set_capture_mute(&id, false) {
                    restored += 1;
                }
            }
            let _ = std::fs::remove_file(&owner_file);
        }
        let m = Self {
            owner_file,
            active: false,
            owned: BTreeSet::new(),
            known: BTreeSet::new(),
            displayed: false,
        };
        (m, restored)
    }

    pub fn is_muted(&self) -> bool {
        self.displayed
    }

    fn persist(&self) {
        if self.owned.is_empty() {
            let _ = std::fs::remove_file(&self.owner_file);
        } else {
            let f = OwnerFile { devices: self.owned.iter().cloned().collect() };
            if let Ok(s) = toml::to_string(&f) {
                let tmp = self.owner_file.with_extension("tmp");
                if std::fs::write(&tmp, s).is_ok() {
                    let _ = std::fs::rename(&tmp, &self.owner_file);
                }
            }
        }
    }

    /// Pulsante / hotkey Mute: porta tutti i microfoni allo stato opposto a
    /// quello mostrato.
    pub fn toggle(&mut self, backend: &dyn AudioBackend) {
        let target = !self.displayed;
        let states = backend.capture_mute_states();
        if target {
            for (id, muted) in &states {
                if !*muted && backend.set_capture_mute(id, true) {
                    self.owned.insert(id.clone());
                }
            }
            self.known = states.into_iter().map(|(id, _)| id).collect();
            self.active = true;
        } else {
            for (id, muted) in &states {
                if *muted {
                    backend.set_capture_mute(id, false);
                }
            }
            self.owned.clear();
            self.known.clear();
            self.active = false;
        }
        self.displayed = target;
        self.persist();
    }

    /// Da chiamare periodicamente: allinea lo stato mostrato a quello reale del
    /// microfono registrato e silenzia i microfoni collegati nel frattempo.
    /// Restituisce lo stato di mute mostrato.
    pub fn poll(&mut self, backend: &dyn AudioBackend, recorded: Option<&str>) -> bool {
        let states = backend.capture_mute_states();
        let mut changed = false;
        if self.active {
            for (id, muted) in &states {
                if !self.known.contains(id) {
                    self.known.insert(id.clone());
                    if !*muted && backend.set_capture_mute(id, true) {
                        self.owned.insert(id.clone());
                        changed = true;
                    }
                }
            }
        }
        let rec_state = recorded.and_then(|r| states.iter().find(|(id, _)| id == r).map(|(_, m)| *m));
        self.displayed = rec_state.unwrap_or(self.active);
        if self.active && !self.displayed {
            // Mute rimosso dall'esterno sul microfono registrato.
            self.active = false;
            if let Some(r) = recorded {
                changed |= self.owned.remove(r);
            }
        }
        if changed {
            self.persist();
        }
        self.displayed
    }

    /// Alla chiusura: rimuove il mute dai soli microfoni silenziati da noi.
    pub fn release(&mut self, backend: &dyn AudioBackend) {
        let states = backend.capture_mute_states();
        for id in &self.owned {
            if states.iter().any(|(d, m)| d == id && *m) {
                backend.set_capture_mute(id, false);
            }
        }
        self.owned.clear();
        self.active = false;
        self.persist();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AppInfo, CaptureHandle, DeviceInfo};
    use crate::fifo::SourceFifo;
    use crate::settings::SourceSetting;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Fake {
        devs: Mutex<Vec<(String, bool)>>,
    }

    impl AudioBackend for Fake {
        fn input_devices(&self) -> Vec<DeviceInfo> {
            vec![]
        }
        fn output_devices(&self) -> Vec<DeviceInfo> {
            vec![]
        }
        fn audio_apps(&self) -> Vec<AppInfo> {
            vec![]
        }
        fn resolve_input_id(&self, _: &SourceSetting) -> Option<String> {
            Some("a".into())
        }
        fn capture_mute_states(&self) -> Vec<(String, bool)> {
            self.devs.lock().unwrap().clone()
        }
        fn set_capture_mute(&self, id: &str, mute: bool) -> bool {
            for d in self.devs.lock().unwrap().iter_mut() {
                if d.0 == id {
                    d.1 = mute;
                }
            }
            true
        }
        fn start_input(&self, _: &SourceSetting, _: Arc<SourceFifo>) -> Box<dyn CaptureHandle> {
            unimplemented!()
        }
        fn start_output(&self, _: &SourceSetting, _: Arc<SourceFifo>) -> Box<dyn CaptureHandle> {
            unimplemented!()
        }
    }

    fn state(b: &Fake, id: &str) -> bool {
        b.devs.lock().unwrap().iter().find(|d| d.0 == id).unwrap().1
    }

    #[test]
    fn mute_all_new_devices_and_crash_recovery() {
        let file = std::env::temp_dir().join(format!("sar-mute-{}.toml", std::process::id()));
        let b = Fake::default();
        *b.devs.lock().unwrap() = vec![("a".into(), false), ("b".into(), true), ("c".into(), false)];
        let (mut m, restored) = MuteManager::new(file.clone(), &b);
        assert_eq!(restored, 0);
        m.toggle(&b);
        assert!(m.is_muted());
        assert!(state(&b, "a") && state(&b, "b") && state(&b, "c"));
        // nuovo microfono collegato durante il mute
        b.devs.lock().unwrap().push(("d".into(), false));
        assert!(m.poll(&b, Some("a")));
        assert!(state(&b, "d"));
        // crash: un nuovo gestore ripristina solo i dispositivi posseduti (non "b")
        let (_m2, restored) = MuteManager::new(file.clone(), &b);
        assert_eq!(restored, 3);
        assert!(!state(&b, "a") && state(&b, "b") && !state(&b, "c") && !state(&b, "d"));
        assert!(!file.exists());
    }

    #[test]
    fn external_unmute_is_reflected() {
        let file = std::env::temp_dir().join(format!("sar-mute2-{}.toml", std::process::id()));
        let b = Fake::default();
        *b.devs.lock().unwrap() = vec![("a".into(), false)];
        let (mut m, _) = MuteManager::new(file.clone(), &b);
        m.toggle(&b);
        b.set_capture_mute("a", false);
        assert!(!m.poll(&b, Some("a")));
        b.set_capture_mute("a", true);
        assert!(m.poll(&b, Some("a")));
        m.toggle(&b);
        assert!(!state(&b, "a"));
        m.release(&b);
        assert!(!file.exists());
    }
}
