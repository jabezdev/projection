# Windows

End users run Windows 10/11; Windows is the acceptance platform. Builds are cross-compiled on Linux with `desktop/tools/build-windows.sh` (see Packaging in [README.md](README.md#packaging)) and tested on the church PC, so test time there is short. Batch fixes into one build per visit.

## Done

- [x] Cross-built NSIS installer from Linux (2026-10-11). First release: `v0.1.0-win-test` on GitHub.
- [x] Native outputs, video and audio run on Windows 10 (confirmed 2026-10-11). There is no webview fallback for the outputs.
- [x] No console window: release builds use the Windows GUI subsystem (`windows_subsystem = "windows"` in `crates/app/src/main.rs`). Built, not yet tested on Windows.
- [x] One taskbar entry: projection and confidence windows skip the taskbar, re-applied each time an output is shown. Built, not yet tested on Windows.

## Fix before a live service

Roadmap phase 1. Built and checked on Linux; not yet tested on Windows. The [church PC checklist](docs/church-pc-checklist.md) covers each on the next visit.

- [/] **Log file.** Without the console, log output would be lost on Windows. Logs go to `%APPDATA%\com.magi.projection\logs\palto.log`, rotated at 5 MB with four older files kept; panics are logged. Settings → Troubleshooting → Open logs folder.
- [/] **Keep the display awake.** While any output is shown, the app calls `SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED)` from a dedicated thread and clears it when outputs are hidden.
- [/] **Crash recovery.** After a crash the next start reopens the last service at the last cue, media paused, outputs hidden.

## Test on the church PC

- [ ] **Display scaling.** Run the laptop at 125% or 150%, ideally with the projector at a different scale. Output sizes and positions are physical pixels; check that each output lands on its display and fills it exactly.
- [ ] **Projector hotplug.** Connect the projector after launch, then unplug and replug it. Windows can renumber displays (`DISPLAY1`/`DISPLAY2`); the saved display assignment must still find the projector.
- [ ] **HDMI audio.** Connecting the projector often makes HDMI the default audio device. `media_engine` reopens the stream on a device change; check it while a video plays, during both plug and unplug.
- [ ] **GPU.** The renderer requests DirectX 12 or Vulkan (`Backends::PRIMARY`, `render_engine/src/lib.rs`). Old integrated GPUs or drivers may offer neither and the outputs would not start. If that happens, add an OpenGL fallback.
- [ ] **Fonts.** Windows ships different fonts from Linux. Check the font list in Output Settings, fonts saved on Linux (fallback behavior), and how lyrics wrap.
- [ ] **Basics.** The app opens; a video plays with sound (proves the FFmpeg DLLs load); outputs go fullscreen on the right displays; closing the operator window exits the app.

## Later (tracked in the roadmap)

- **Media paths.** Services refer to media by absolute path, so a library copied from Linux cannot find its files on Windows. Copy the media folder onto the church PC, then Library → Missing → Search a folder… relinks everything by file name. New imports are copied into the library's own `media` folder.
- **Installer name and version.** Done (phase 2): the product is **PALTO** (`PALTO.exe`, Start menu, window titles) and `build-windows.sh` bumps the `0.MINOR.BUILD` version every build, so installers upgrade in place. Check the upgrade on the next visit.
- **Updates.** Installed by hand. Planned: auto-update with confirmation and rollback (phase 11).
- **Code signing.** Decided to skip for now: SmartScreen warns (More info → Run anyway) and Defender occasionally flags unsigned installers.
- **Remote over the network.** Done (phase 10): Settings → Remote turns on LAN access (off by default: the server binds `127.0.0.1` and no firewall prompt appears). The installer adds an inbound Windows Firewall rule for `PALTO.exe` on Private and Domain networks, asking for administrator permission once (updates find the rule and do not ask); uninstalling removes it. On a network Windows classifies as Public, phones cannot connect until it is set to Private. Phones pair with a code; `palto.local` is advertised over mDNS. Check on the next visit.
