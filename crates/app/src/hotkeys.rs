//! Hotkey globali (attivi anche quando la finestra non è in primo piano).

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use simpleaudiorec_core::settings::{Action, Hotkeys};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::mpsc::{Receiver, channel};

pub struct HotkeyService {
    manager: Option<GlobalHotKeyManager>,
    registered: Vec<HotKey>,
    map: HashMap<u32, Action>,
    rx: Receiver<u32>,
}

impl HotkeyService {
    /// Va creato sul thread principale (quello con il ciclo dei messaggi).
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = channel();
        GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
            if e.state == HotKeyState::Pressed {
                let _ = tx.send(e.id);
                ctx.request_repaint();
            }
        }));
        Self { manager: GlobalHotKeyManager::new().ok(), registered: Vec::new(), map: HashMap::new(), rx }
    }

    pub fn unregister_all(&mut self) {
        if let Some(m) = &self.manager {
            let _ = m.unregister_all(&self.registered);
        }
        self.registered.clear();
        self.map.clear();
    }

    /// Registra le combinazioni; restituisce quelle non disponibili.
    pub fn register(&mut self, keys: &Hotkeys) -> Vec<String> {
        self.unregister_all();
        let mut failed = Vec::new();
        let Some(m) = &self.manager else { return failed };
        for a in Action::ALL {
            let text = keys.get(a);
            if text.is_empty() {
                continue;
            }
            match HotKey::from_str(text) {
                Ok(hk) if m.register(hk).is_ok() => {
                    self.map.insert(hk.id(), a);
                    self.registered.push(hk);
                }
                _ => failed.push(text.to_string()),
            }
        }
        failed
    }

    /// Azioni premute dall'ultimo controllo.
    pub fn poll(&self) -> Vec<Action> {
        self.rx.try_iter().filter_map(|id| self.map.get(&id).copied()).collect()
    }
}

/// Converte un tasto premuto nella UI in una combinazione (es. `Ctrl+Alt+R`).
/// Serve almeno un modificatore tra Ctrl, Alt e Shift.
pub fn combo_from(key: egui::Key, m: egui::Modifiers) -> Option<String> {
    let name = key.name();
    let single = name.len() == 1 && name.chars().all(|c| c.is_ascii_alphanumeric());
    let fkey = name.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| (1..=12).contains(&n));
    if !(single || fkey) {
        return None;
    }
    let name = name.to_ascii_uppercase();
    if !(m.ctrl || m.alt || m.shift) {
        return None;
    }
    let mut s = String::new();
    if m.ctrl {
        s.push_str("Ctrl+");
    }
    if m.alt {
        s.push_str("Alt+");
    }
    if m.shift {
        s.push_str("Shift+");
    }
    s.push_str(&name);
    HotKey::from_str(&s).ok().map(|_| s)
}
