# SimpleAudioRec — Requirements specification v1

*Versione italiana (documento di riferimento): [REQUISITI.md](REQUISITI.md)*

| | |
|---|---|
| Document version | 0.3 |
| Date | 2026-11-01 |
| v1 platform | Windows 11 x64 |
| License | GPLv3 |
| Repository | `Nahlt/SimpleAudioRec` |

Priorities: **M** = Must (required in v1), **S** = Should (nice to have in v1, done if feasible without compromising the Musts), **V2** = deferred.

The user interface is available in Italian and English: labels quoted in this document are the English ones.

### Version history

| Version | Changes |
|---|---|
| 0.1 | First draft. |
| 0.2 | App name: *SimpleAudioRec* (was *SimpleRec*). "Default" follows the **communications** device (RF-01/02/03). Mute applies to **all** active microphones (RF-20, RF-23, RF-24). Added mixed mono format, the "create both formats" option and the `_stereo`/`_mono` suffix in the file name (RF-44a/b, RF-41). **Loudness** normalization instead of peak normalization (RF-45). Added automatic saving (RF-48), **bookmarks** (§3.8), single instance, window position and ID3 tags. Added build pipeline and distribution (§6.1). |
| 0.3 | Neutral icon (a document), with no reference to recording (§4). Releases are published from the GitHub website and the pipeline attaches the zip (§6.1). Added English versions of the requirements and user guide. |

---

## 1. Purpose

SimpleAudioRec is a minimal desktop application for recording the PC's audio during online meetings (Microsoft Teams first and foremost, but also Zoom, Meet, Webex, etc.). It records at the same time:

- the **output** audio (what you hear: the other participants);
- the **input** audio from the microphone (your own voice).

The result is an MP3 file: stereo with the two streams on separate channels (designed for transcription) and/or mixed mono (designed for listening back).

## 2. Context and constraints

| ID | Constraint |
|---|---|
| C-01 | Personal use, single user. |
| C-02 | **Portable** application: a single `SimpleAudioRec.exe`, no installer, no runtime to install, no administrator rights required. |
| C-03 | Top priority: reliable operation on Windows 11. The code must be structured for a future Linux port (V2), without sacrificing quality on Windows. |
| C-04 | GPLv3 license. All dependencies must be GPLv3-compatible. |
| C-05 | No recording-consent reminder. |
| C-06 | Teams' local API for third-party apps is not available in the user's environment: integration with Teams' mute is not possible. Mute is handled at system level (§3.3). |

## 3. Functional requirements

### 3.1 Audio sources

| ID | Pri | Requirement |
|---|---|---|
| RF-01 | M | The user chooses the **input device** (microphone) from the settings menu. Options: "Default communications device", "System default device" + list of active capture devices. **Default: "Default communications device"**, because Teams and similar tools normally use that device. |
| RF-02 | M | The user chooses the **output device** to record (via loopback) from the settings menu. Options: "Default communications device", "System default device" + list of active playback devices. **Default: "Default communications device"**. |
| RF-03 | M | With one of the "Default…" options, if Windows changes the default device for the corresponding role during recording (e.g. Bluetooth headphones are connected), SimpleAudioRec automatically switches to the new device without interrupting the recording. Any transition seconds are filled with silence, keeping the time alignment. |
| RF-04 | M | If an explicitly chosen device is disconnected during recording, its channel continues in silence, the UI shows a warning and capture resumes automatically if the device becomes available again. |
| RF-05 | S | For the output, as an alternative to a device, the user can choose **"Only the application"** and select it from a list of processes with active audio (e.g. Teams). Audio is captured from the application's entire process tree (needed for Teams, which plays audio from child processes). |
| RF-06 | M | Changing sources from the settings menu is disabled during a recording (only the automatic following of RF-03 remains active). |

### 3.2 Recording

| ID | Pri | Requirement |
|---|---|---|
| RF-10 | M | **Start**: starts recording from a button or hotkey. |
| RF-11 | M | **Pause / Resume**: while paused, audio is not recorded; on resume the file continues with no gaps (the paused part simply does not exist in the file). |
| RF-12 | M | **Stop**: ends the recording and starts saving (§3.5). |
| RF-13 | M | No limit on recording length. |
| RF-14 | M | A timer shows the recorded duration (pauses excluded) in `hh:mm:ss` format. |
| RF-15 | M | In the raw recording the **left channel = microphone** and the **right channel = output audio**. Stereo sources are converted to mono before being placed on their channel. |
| RF-16 | M | The two channels stay aligned in time for the whole recording, even when the two devices have different clocks (see RNF-04). When no audio is playing, the right channel contains silence (in loopback mode Windows sends no data when there is no audio: this must be compensated). |
| RF-17 | M | If the window is closed during a recording, the user is asked: "Recording in progress. Stop and save?" → *Stop and save* / *Cancel*. |
| RF-18 | M | A hotkey that does not apply to the current state is silently ignored (e.g. *Pause* while stopped, *Start* during a recording). |

### 3.3 Microphone mute

| ID | Pri | Requirement |
|---|---|---|
| RF-20 | M | A button and a global hotkey toggle a **system-level microphone mute** on **all active capture devices**, not only the recorded one. This way the mute is effective whichever microphone Teams (or another tool) uses, even if different from the one recorded by SimpleAudioRec. Effect: the microphone is silenced for all applications and for the recording itself. A microphone connected while SimpleAudioRec's mute is active is muted as well. |
| RF-21 | M | Mute always works while SimpleAudioRec is open, also outside a recording. |
| RF-22 | M | While muted, the microphone channel contains silence (time alignment preserved). |
| RF-23 | M | The UI reflects the mute state of the **recorded microphone**, including changes made externally (hardware mute key, Windows quick settings). On the next press of Mute all microphones are aligned to the same state. |
| RF-24 | M | SimpleAudioRec stores on disk which devices it muted itself ("ownership" of the mute). On exit it removes the mute only from those devices; a mute applied externally beforehand is not touched. If the app exited abnormally leaving microphones muted, on the next start the mute is removed from those devices and the user is notified. |
| RF-25 | — | Note (accepted limitation): a mute applied with Teams' own button (or another tool's) cannot be detected; in that case your voice is still recorded. To exclude it from the recording, SimpleAudioRec's mute must be used. Teams is not required to show the mute state. |

### 3.4 Status indicators

| ID | Pri | Requirement |
|---|---|---|
| RF-30 | M | **Red dot**: recording in progress. While paused the dot is off/grey with the label "Paused". |
| RF-31 | M | **Green dot**: lights up when audio is detected from the output (level above a threshold, e.g. −50 dBFS, with a short ~150 ms hold to avoid flicker). |
| RF-32 | M | **Orange dot**: lights up when audio is detected from the microphone (same logic). While muted it stays off and a crossed-out microphone icon appears next to it. |
| RF-33 | M | The green and orange indicators are active during recording and during pause (to check the audio before resuming). No VU meter, no volume control. |
| RF-34 | S | With the window minimized, the state is visible from the taskbar: a red-dot overlay on the icon and the timer in the window title. |

### 3.5 Saving

| ID | Pri | Requirement |
|---|---|---|
| RF-40 | M | On Stop the **save panel** appears in the main window, with: a "Create both formats (stereo and mono)" checkbox prefilled from the settings; the list of bookmarks with editable descriptions (§3.8); *Save…* and *Discard* buttons. *Save…* opens the standard Windows "Save as" dialog. |
| RF-41 | M | Proposed name: recording start date and time + format, `YYYY-MM-DD_hh-mm_stereo.mp3` or `YYYY-MM-DD_hh-mm_mono.mp3` (e.g. `2026-11-01_14-30_stereo.mp3`). With "both formats" the user chooses a single name in the dialog; the second file is created next to it, with the same name and the other format's suffix. |
| RF-42 | M | Proposed folder: the last one used (on first run: Documents). |
| RF-43 | M | If the user cancels the save dialog or presses *Discard*: confirmation "Discard the recording?" → *Yes* (deleted) / *No* (back to the save panel). |
| RF-44 | M | MP3 format, CBR, 48 kHz. Configurable bitrate: 64 / 96 / 128 / 160 / 192 kbps. **Default 96 kbps** (~43 MB/hour in stereo). |
| RF-44a | M | **Default format** chosen in the settings: **"Separate stereo"** (default; left channel = microphone, right = output, **plain stereo, not joint stereo**, to avoid crosstalk between channels; suited to transcription) or **"Mixed mono"** (sum of the two channels; suited to listening on headphones). Mono uses half the configured bitrate, with a minimum of 48 kbps (e.g. 96 → 48 kbps). |
| RF-44b | M | **"Create both formats"** setting (default: off), which prefills the matching checkbox in the save panel (RF-40). |
| RF-45 | M | Optional **loudness normalization**, enabled from the settings menu (default: on). Applied **separately to each channel**: integrated loudness (ITU-R BS.1770 / EBU R128) brought to **−16 LUFS**, gain limited between −10 and +15 dB, a −1 dBFS peak limiter that acts only on peaks above the threshold (no dynamic compression). A silent channel (below the gating threshold) is not amplified. In mono, the sum of the normalized channels goes through the limiter again. |
| RF-46 | M | Encoding happens at the end; a progress bar is shown for long recordings. |
| RF-47 | M | Warning when the free space on the temporary-files disk falls below the required threshold (< 1 GB or < 30 minutes of estimated recording). The recording is not interrupted. |
| RF-48 | M | **"Save automatically to the last folder"** setting (default: off). When on, neither the panel nor the dialog appears on Stop: files are saved with the proposed name, in the formats set in the settings, in the last folder used. If a file with the same name already exists, a suffix ` (2)`, ` (3)`… is added. At the end a message in the window shows where the file was saved. |
| RF-49 | M | Each MP3 contains **ID3v2 tags**: title `Recording YYYY-MM-DD hh:mm` (`Registrazione …` with the Italian interface), date, duration, software `SimpleAudioRec`, bookmarks as chapters (§3.8). |

### 3.6 Robustness and recovery

| ID | Pri | Requirement |
|---|---|---|
| RF-50 | M | During recording the audio is written progressively to an uncompressed temporary file (16-bit PCM stereo 48 kHz, ~690 MB/hour), updated so that it stays readable even after a crash. Bookmarks are saved progressively next to the temporary file. |
| RF-51 | M | At startup, if an unsaved recording is found: "An unsaved recording from <date> (<duration>) was found. Recover it?" → *Recover* (goes to the save panel §3.5, bookmarks included) / *Discard* (with confirmation). |
| RF-52 | M | After a successful save or a confirmed discard, the temporary files are deleted. |
| RF-53 | M | **Single instance**: if the exe is started again while SimpleAudioRec is already open, the existing window is brought to the front and the second instance exits (avoids conflicts on hotkeys and mute). |

### 3.7 Global hotkeys

Combinations active even when SimpleAudioRec is not in the foreground (e.g. while using Teams).

| ID | Pri | Action | Default |
|---|---|---|---|
| RF-60 | M | Start recording | `Ctrl+Alt+R` |
| RF-61 | M | Pause / Resume | `Ctrl+Alt+P` |
| RF-62 | M | Stop | `Ctrl+Alt+S` |
| RF-63 | M | Mute / Unmute microphone | `Ctrl+Alt+M` |
| RF-65 | M | Add bookmark | `Ctrl+Alt+B` |
| RF-64 | M | Hotkeys can be changed from the settings menu. If a combination is already taken by another application, the user is warned. |

### 3.8 Bookmarks

| ID | Pri | Requirement |
|---|---|---|
| RF-80 | M | During a recording (also while paused) the user adds a **bookmark** with the *Mark* button or the `Ctrl+Alt+B` hotkey. The bookmark records the current point of the recording (recorded time, pauses excluded). |
| RF-81 | M | Each bookmark gets an automatic description ("Bookmark 1", "Bookmark 2"…), editable in the save panel. When pressed, the UI gives brief visual feedback (e.g. "🔖 3" next to the timer). |
| RF-82 | M | On saving, if there is at least one bookmark: (a) bookmarks are written as **ID3 chapters** (`CHAP`/`CTOC` frames) in every MP3 created; (b) a text file is created next to the MP3, with the same base name and the suffix `_bookmarks.txt` (`_segnalibri.txt` with the Italian interface), e.g. `2026-11-01_14-30_bookmarks.txt`, one line per bookmark in the format `hh:mm:ss  description`. |

### 3.9 Settings

| ID | Pri | Requirement |
|---|---|---|
| RF-70 | M | Settings menu opened from a gear icon in the main window. It contains: input device, output source (RF-02/RF-05), default format (RF-44a), create both formats (RF-44b), bitrate, normalization, automatic saving (RF-48), hotkeys, language. |
| RF-71 | M | Settings (including the last save folder and the window position) are stored in a `SimpleAudioRec.toml` file next to the exe; if the folder is not writable, in `%APPDATA%\SimpleAudioRec\`. The temporary files of RF-50 and the mute ownership state (RF-24) follow the same rule (`recovery` subfolder). |
| RF-72 | M | Interface language: Italian and English. Default: system language (English fallback), changeable. |

## 4. User interface

- Normal, compact, fixed-size window; **not** always on top; can be minimized to the taskbar; **no** system tray icon; no automatic start with Windows.
- The window position is remembered between runs (if the screen is no longer available, the window is centered).
- Original, neutral app icon (a document on a grey background), Windows 11 style, with no reference to audio or recording.
- Main window content:

```
┌──────────────────────────────────────┐
│  ● REC   00:42:17   🔖 3          ⚙  │   ● red = recording
│                                      │
│  ● Output        ● Microphone  🎙/🔇  │   green / orange + mute state
│                                      │
│   [ ⏺ Rec ]  [ ⏸ Pause ]  [ ⏹ Stop ]  │
│        [ 🔇 Mute ]   [ 🔖 Mark ]      │
└──────────────────────────────────────┘
```

- Button states: *Rec* enabled only when stopped; *Pause* (which becomes *Resume*), *Stop* and *Mark* enabled only during recording; *Mute* always enabled.
- Save panel (after Stop, RF-40):

```
┌──────────────────────────────────────┐
│  Recording finished — 00:42:17       │
│  [x] Create both formats             │
│  Bookmarks:                          │
│   00:05:12  [Bookmark 1           ]  │
│   00:31:40  [Budget decision      ]  │
│                                      │
│        [ Save… ]   [ Discard ]       │
└──────────────────────────────────────┘
```

## 5. Non-functional requirements

| ID | Requirement |
|---|---|
| RNF-01 | A single `.exe` for Windows x64, with no external dependencies (libraries and C runtime statically linked), indicative size ≤ 15 MB. |
| RNF-02 | Startup in less than 1 s. |
| RNF-03 | Resource usage while recording: average CPU < 3% on a modern PC, RAM < 100 MB. |
| RNF-04 | Channel desynchronization < 100 ms after 3 hours of recording (compensation of clock drift between devices through adaptive resampling). |
| RNF-05 | No audible audio loss (drop-outs) under normal load. |
| RNF-06 | No telemetry and no network connection. |
| RNF-07 | The exe is not digitally signed: the SmartScreen warning on first run is accepted ("More info → Run anyway"). On company PCs, policies (AppLocker, WDAC) may require approval from IT. |

## 6. Technical architecture

**Stack**: Rust, interface built with `egui`/`eframe`.

Modular structure, with audio behind an abstraction for the future Linux port:

| Module | Responsibility |
|---|---|
| `core` | Recording state machine, two-channel mixing, synchronization/resampling, temporary file and recovery, bookmarks, loudness normalization, mono mix, MP3 encoding, ID3 tags, settings, translations. Platform-independent. |
| `audio` (trait `AudioBackend`) | Device/application listing, microphone capture, loopback capture, per-process capture, system mute, device/mute change notifications. |
| `audio::wasapi` | Windows implementation: WASAPI (loopback, process loopback), `IAudioEndpointVolume` for mute, `IMMNotificationClient` for device changes. |
| `audio::pipewire` | V2 — Linux implementation. |
| `app` | egui UI, global hotkeys, dialogs, single instance, taskbar overlay. |

Planned dependencies (all GPLv3-compatible): `eframe`/`egui` (MIT/Apache-2.0), `windows` (MIT/Apache-2.0), LAME through a Rust binding, statically linked (LGPL), `global-hotkey` (MIT/Apache-2.0), `rfd` for dialogs (MIT), `serde` + `toml` (MIT/Apache-2.0), `ebur128` for loudness measurement (MIT), `id3` for tags (MIT). Adaptive resampling (conversion to 48 kHz and drift compensation) is implemented in the `core` module (polyphase sinc with a Kaiser window) instead of using `rubato`, to have a small component tested on this use case. The interface uses eframe's OpenGL backend (`glow`), which is lighter and faster to start. Third-party licenses are collected in `THIRD_PARTY_NOTICES.txt`, distributed with the exe.

### 6.1 Build and distribution

- Development with direct commits on the `main` branch.
- Local verification builds (also cross-compiled from Linux) and unit tests of the `core` module.
- **GitHub Actions** on `windows-latest` runners (target `x86_64-pc-windows-msvc`, static CRT):
  - on every push to `main` and manually: tests + build, exe downloadable from the run's *Artifacts*;
  - when a **Release** is published from the GitHub website (with a new tag, e.g. `v0.1.0`): the zip (`SimpleAudioRec.exe`, `LEGGIMI.txt`, `README.txt`, `LICENSE`, `THIRD_PARTY_NOTICES.txt`) is attached to the Release.
- Functional tests with real audio are done on a Windows 11 PC (the runners have no audio devices).

## 7. Out of scope for v1 / roadmap

| Pri | Feature |
|---|---|
| V2 | Automatic start of recording when a meeting begins. |
| V2 | Linux version (PipeWire/PulseAudio). |
| V2 | Integration with the Teams API (real mute state) if it becomes available. |
| V2 | Echo cancellation (use with speakers instead of headphones). |
| — | Transcription: out of scope; the separate stereo format is designed for external tools. |

## 8. Risks and notes

| Risk | Mitigation |
|---|---|
| Use with speakers: the microphone also picks up remote voices (echo in the left channel). | Headphones recommended; echo cancellation in V2. |
| Bluetooth headphones: when the microphone is opened, Windows may switch to the "hands-free" profile (lower quality) and change device. | Handled by RF-03; known hardware/driver limitation. |
| Teams configured on a specific device different from the default communications device: SimpleAudioRec would record the wrong device. | Default "Default communications device" (RF-01/02); explicit device choice in the settings; mute applies to all microphones anyway (RF-20). |
| Mute from Teams' own button cannot be detected. | Use SimpleAudioRec's mute (RF-25). |
| Microphones left muted at system level after a crash. | RF-24 (mute ownership saved to disk). |
| Unsigned exe blocked on managed company PCs. | RNF-07; signing may be evaluated in the future. |
| Per-application capture (RF-05) on processes with audio in child processes. | Capture of the entire process tree; S requirement, verified during development. |

## 9. Acceptance criteria

1. Starting a recording during a test Teams meeting, the saved `_stereo.mp3` file contains your own voice **only** on the left channel and the participants' voices **only** on the right; the `_mono.mp3` file (if created) contains both.
2. The green and orange indicators light up when speaking or playing audio, and stay off in silence.
3. `Ctrl+Alt+M` with Teams in the foreground: the participants no longer hear the user and the left channel is silent from that moment; pressing it again restores everything. It also works if Teams uses a microphone different from the recorded one.
4. A 1-minute pause in a 5-minute recording produces a 4-minute file, with no gaps.
5. Disconnecting/connecting the headphones during recording, the recording continues and follows the new default device.
6. Force-killing the process during a recording, recovery is offered on restart and the recovered file contains the audio (and the bookmarks) up to the moment of the crash; the microphones do not stay muted.
7. In a 2-hour recording the channels stay synchronized (offset < 100 ms, checked with a reference signal at the start and at the end).
8. The exe works when copied to any folder (even a USB stick) on a clean Windows 11, with no additional installations.
9. Three bookmarks added during a recording appear in the bookmarks text file with the correct times (±1 s) and as chapters in a player that supports them.
10. With "Save automatically" on, Stop saves the files without dialogs to the last folder, without overwriting existing files.
11. Starting the exe a second time brings up the already open window and does not create a second instance.
