# SimpleAudioRec

Applicazione desktop minimale per registrare i meeting online (Microsoft Teams, Zoom, Meet, Webex…) su Windows 11: registra insieme **l'audio in uscita** (gli altri partecipanti) e **il microfono** (la propria voce) e salva un MP3.

- **Stereo separato** (microfono a sinistra, output a destra), pensato per la trascrizione, e/o **mono mixato** per il riascolto.
- **Mute di sistema** su tutti i microfoni con un tasto globale: Teams non sente e la voce non viene registrata.
- **Segnalibri** durante la registrazione (capitoli ID3 + file di testo).
- Pausa senza buchi, normalizzazione di loudness per canale (−16 LUFS), recupero dopo un crash.
- Un solo `.exe` portabile, nessuna installazione, nessuna connessione di rete.

Requisiti completi: [docs/REQUISITI.md](docs/REQUISITI.md). Istruzioni per l'utente: [LEGGIMI.txt](LEGGIMI.txt).

## Download

- Ultima build di `main`: tab **Actions → Build Windows →** ultima esecuzione **→ Artifacts**.
- Versioni: pagina **Releases** (create a ogni tag `v*`).

L'eseguibile non è firmato: al primo avvio SmartScreen chiede conferma (*Ulteriori informazioni → Esegui comunque*).

## Scorciatoie predefinite

| Azione | Tasti |
|---|---|
| Avvia | `Ctrl+Alt+R` |
| Pausa / Riprendi | `Ctrl+Alt+P` |
| Stop | `Ctrl+Alt+S` |
| Mute microfono | `Ctrl+Alt+M` |
| Segnalibro | `Ctrl+Alt+B` |

## Struttura

| Percorso | Contenuto |
|---|---|
| `crates/core` | Logica indipendente dalla piattaforma: mixer con compensazione della deriva, ricampionatore, WAV di recupero, loudness e limiter, codifica MP3 (LAME), tag ID3, mute, impostazioni, traduzioni. Testata su qualsiasi OS. |
| `crates/app` | Applicazione Windows: interfaccia egui, backend WASAPI (cattura, loopback, process loopback, mute), hotkey globali, istanza singola, overlay della barra delle applicazioni. |

## Build

```powershell
cargo test -p simpleaudiorec_core
cargo build --release -p simpleaudiorec   # target\release\SimpleAudioRec.exe
```

Su Windows serve Rust stabile con il toolchain MSVC; il runtime C è collegato staticamente (`.cargo/config.toml`). La pipeline GitHub (`.github/workflows/build-windows.yml`) compila su `windows-latest` a ogni push su `main` e pubblica una Release per ogni tag `v*`.

## Licenza

GPL-3.0-or-later. Componenti di terze parti in [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt).
