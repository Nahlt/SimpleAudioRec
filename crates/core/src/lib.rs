//! SimpleAudioRec — logica indipendente dalla piattaforma.
//!
//! Contiene la macchina a stati della registrazione, il mixing su due canali con
//! compensazione della deriva, il file temporaneo e il recupero, i segnalibri,
//! la normalizzazione di loudness, la codifica MP3, le impostazioni e le traduzioni.
//! L'accesso all'audio del sistema passa dal trait [`audio::AudioBackend`].

pub mod audio;
pub mod export;
pub mod fifo;
pub mod i18n;
pub mod limiter;
pub mod mixer;
pub mod mute;
pub mod naming;
pub mod paths;
pub mod recorder;
pub mod resampler;
pub mod session;
pub mod settings;
pub mod wav;

/// Nome dell'applicazione (finestra, file di impostazioni, cartelle).
pub const APP_NAME: &str = "SimpleAudioRec";
/// Versione dell'applicazione.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Frequenza di campionamento interna e di uscita.
pub const SAMPLE_RATE: u32 = 48_000;

/// Formatta una durata in frame come `hh:mm:ss`.
pub fn format_hms(frames: u64) -> String {
    let secs = frames / SAMPLE_RATE as u64;
    format!("{:02}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
}

/// Blocca un mutex ignorando l'eventuale avvelenamento (un thread andato in panic
/// non deve impedire il salvataggio della registrazione).
pub fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    #[test]
    fn hms() {
        assert_eq!(super::format_hms(0), "00:00:00");
        assert_eq!(super::format_hms(48_000 * 3725), "01:02:05");
    }
}
