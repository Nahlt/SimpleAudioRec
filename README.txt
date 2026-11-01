SimpleAudioRec - portable version for Windows 11
================================================
(Versione italiana: LEGGIMI.txt)

STARTING
  Double-click SimpleAudioRec.exe. No installation is needed.

  On first launch Windows may show "Windows protected your PC":
  click "More info" and then "Run anyway".
  On company PCs, policies may require approval from IT.

QUICK START
  1. Start recording with "Rec" (or Ctrl+Alt+R, even while using Teams).
  2. "Pause" (Ctrl+Alt+P) suspends: the paused part does not end up in the file.
  3. "Mute" (Ctrl+Alt+M) mutes ALL microphones at system level:
     Teams can't hear you and your voice is not recorded. Press again to unmute.
     Note: SimpleAudioRec cannot see Teams' own mute button.
  4. "Mark" (Ctrl+Alt+B) adds a bookmark at the current position.
  5. "Stop" (Ctrl+Alt+S) ends the recording: choose whether to create both formats,
     rename the bookmarks and press "Save…".

  Indicators: green = audio coming from the other participants (output),
  orange = audio coming from your microphone, red = recording in progress.

FILES CREATED
  2026-11-01_14-30_stereo.mp3   microphone on the left, participants on the right
                                (best for transcription)
  2026-11-01_14-30_mono.mp3     everything mixed (best for listening back)
  2026-11-01_14-30_bookmarks.txt   list of bookmarks (also stored as chapters in the MP3)

  With the interface in Italian the bookmarks file is named "_segnalibri.txt".
  The default format and the "both formats" option are in the settings (gear icon).

TIPS
  - Use headphones: with speakers the microphone also picks up the other people's voices.
  - By default SimpleAudioRec records the "default communications" devices,
    the ones Teams normally uses. If you selected a specific device in Teams,
    select the same one in the settings.
  - To record only Teams' audio (without notifications or other PC sounds),
    choose "Only the application: ms-teams.exe" as output while Teams is in a call.

RECOVERY
  While recording, the audio is written to disk continuously: if the PC or the
  app shuts down unexpectedly, recovery is offered the next time you start it.

SETTINGS
  They are stored in the file SimpleAudioRec.toml in this folder (or in
  %APPDATA%\SimpleAudioRec if the folder is read-only), together with the
  "recovery" subfolder. To restore the defaults, just delete the file.

LICENSES
  GPL v3 (see LICENSE). Third-party components: THIRD_PARTY_NOTICES.txt.
