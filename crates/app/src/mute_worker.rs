//! Thread che gestisce il mute di sistema: applica i comandi e controlla
//! periodicamente lo stato reale dei microfoni, senza bloccare l'interfaccia.

use simpleaudiorec_core::audio::AudioBackend;
use simpleaudiorec_core::mute::MuteManager;
use simpleaudiorec_core::settings::SourceSetting;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

enum Cmd {
    Toggle,
    SetInput(SourceSetting),
    Release(Sender<()>),
}

pub struct MuteWorker {
    tx: Sender<Cmd>,
    muted: Arc<AtomicBool>,
    /// Microfoni da cui è stato rimosso un mute rimasto da un crash (RF-24).
    pub restored: Arc<AtomicUsize>,
}

impl MuteWorker {
    pub fn start(
        backend: Arc<dyn AudioBackend>,
        owner_file: PathBuf,
        input: SourceSetting,
        on_change: impl Fn() + Send + 'static,
    ) -> Self {
        let (tx, rx) = channel();
        let muted = Arc::new(AtomicBool::new(false));
        let restored = Arc::new(AtomicUsize::new(0));
        let (m, r) = (muted.clone(), restored.clone());
        std::thread::Builder::new()
            .name("mute".into())
            .spawn(move || run(backend, owner_file, input, rx, m, r, on_change))
            .expect("thread mute");
        Self { tx, muted, restored }
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    pub fn toggle(&self) {
        let _ = self.tx.send(Cmd::Toggle);
    }

    pub fn set_input(&self, s: SourceSetting) {
        let _ = self.tx.send(Cmd::SetInput(s));
    }

    /// Rimuove il mute applicato da SimpleAudioRec e attende la conferma.
    pub fn release(&self) {
        let (tx, rx) = channel();
        if self.tx.send(Cmd::Release(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(3));
        }
    }
}

fn run(
    backend: Arc<dyn AudioBackend>,
    owner_file: PathBuf,
    mut input: SourceSetting,
    rx: Receiver<Cmd>,
    muted: Arc<AtomicBool>,
    restored: Arc<AtomicUsize>,
    on_change: impl Fn(),
) {
    let (mut mgr, n) = MuteManager::new(owner_file, backend.as_ref());
    if n > 0 {
        restored.store(n, Ordering::Relaxed);
        on_change();
    }
    let mut recorded = backend.resolve_input_id(&input);
    let mut polls = 0u32;
    loop {
        match rx.recv_timeout(Duration::from_millis(300)) {
            Ok(Cmd::Toggle) => mgr.toggle(backend.as_ref()),
            Ok(Cmd::SetInput(s)) => {
                input = s;
                recorded = backend.resolve_input_id(&input);
            }
            Ok(Cmd::Release(done)) => {
                mgr.release(backend.as_ref());
                let _ = done.send(());
                return;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return,
        }
        polls += 1;
        if polls % 4 == 0 {
            // Il predefinito può cambiare (cuffie collegate): ricalcola ogni ~1 s.
            recorded = backend.resolve_input_id(&input);
        }
        let m = mgr.poll(backend.as_ref(), recorded.as_deref());
        if muted.swap(m, Ordering::Relaxed) != m {
            on_change();
        }
    }
}
