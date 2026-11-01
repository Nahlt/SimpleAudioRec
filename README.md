# SimpleAudioRec

A minimal desktop app for recording online meetings (Microsoft Teams, Zoom, Meet, Webex…) on Windows 11. It records **the output audio** (the other participants) and **the microphone** (your own voice) together and saves an MP3.

- **Separate stereo** (microphone on the left, output on the right), designed for transcription, and/or **mixed mono** for listening back.
- **System-wide mute** on all microphones with a global hotkey: Teams can't hear you and your voice isn't recorded.
- **Bookmarks** during recording (ID3 chapters + text file).
- Gapless pause, per-channel loudness normalization (−16 LUFS), recovery after a crash.
- A single portable `.exe`: no installation, no network connection.

Full requirements: [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md) (Italian original: [REQUISITI.md](docs/REQUISITI.md)). User guide: [README.txt](README.txt) (Italian: [LEGGIMI.txt](LEGGIMI.txt)).

## Download

- Latest `main` build: **Actions → Build Windows →** latest run **→ Artifacts**.
- Versions: **Releases** page (the zip is attached automatically when a release is published).

The executable is not signed: on first launch SmartScreen asks for confirmation (*More info → Run anyway*).

## Default shortcuts

| Action | Keys |
|---|---|
| Start | `Ctrl+Alt+R` |
| Pause / Resume | `Ctrl+Alt+P` |
| Stop | `Ctrl+Alt+S` |
| Mute microphone | `Ctrl+Alt+M` |
| Bookmark | `Ctrl+Alt+B` |

## Structure

| Path | Contents |
|---|---|
| `crates/core` | Platform-independent logic: mixer with clock-drift compensation, resampler, recovery WAV, loudness and limiter, MP3 encoding (LAME), ID3 tags, mute, settings, translations. Tested on any OS. |
| `crates/app` | Windows application: egui interface, WASAPI backend (capture, loopback, process loopback, mute), global hotkeys, single instance, taskbar overlay. |

## Build

```powershell
cargo test -p simpleaudiorec_core
cargo build --release -p simpleaudiorec   # target\release\SimpleAudioRec.exe
```

On Windows you need stable Rust with the MSVC toolchain; the C runtime is linked statically (`.cargo/config.toml`). The GitHub pipeline (`.github/workflows/build-windows.yml`) builds on `windows-latest` on every push to `main` and attaches the zip to every published Release.

## License

GPL-3.0-or-later. Third-party components in [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt).
