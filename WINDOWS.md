# Windows

End users run Windows 10/11; Windows is the acceptance platform. Builds are cross-compiled on Linux with `desktop/tools/build-windows.sh` (see Packaging in [README.md](README.md#packaging)) and tested on the church PC, so test time there is short. Batch fixes into one build per visit.

## Done

- [x] Cross-built NSIS installer from Linux (2026-10-11). First release: `v0.1.0-win-test` on GitHub.
- [x] Native outputs, video and audio run on Windows 10 (confirmed 2026-10-11). There is no webview fallback for the outputs.
- [x] No console window: release builds use the Windows GUI subsystem (`windows_subsystem = "windows"` in `crates/app/src/main.rs`). Built, not yet tested on Windows.
- [x] One taskbar entry: projection and confidence windows skip the taskbar, re-applied each time an output is shown. Built, not yet tested on Windows.

## Fix before a live service

These are also roadmap phase 1, together with crash recovery and an "Open logs folder" button.


- [ ] **Log file.** Without the console, `env_logger` output is lost on Windows, so a failure at church leaves nothing to inspect. Write logs to `%APPDATA%\com.magi.projection\logs\`, rotated by size or date.
- [ ] **Keep the display awake.** Nothing stops Windows from blanking the display, starting the screensaver or sleeping during a long stretch without input. While any output is shown, call `SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED)`; clear it when outputs are hidden.

## Test on the church PC

- [ ] **Display scaling.** Run the laptop at 125% or 150%, ideally with the projector at a different scale. Output sizes and positions are physical pixels; check that each output lands on its display and fills it exactly.
- [ ] **Projector hotplug.** Connect the projector after launch, then unplug and replug it. Windows can renumber displays (`DISPLAY1`/`DISPLAY2`); the saved display assignment must still find the projector.
- [ ] **HDMI audio.** Connecting the projector often makes HDMI the default audio device. `media_engine` reopens the stream on a device change; check it while a video plays, during both plug and unplug.
- [ ] **GPU.** The renderer requests DirectX 12 or Vulkan (`Backends::PRIMARY`, `render_engine/src/lib.rs`). Old integrated GPUs or drivers may offer neither and the outputs would not start. If that happens, add an OpenGL fallback.
- [ ] **Fonts.** Windows ships different fonts from Linux. Check the font list in Output Settings, fonts saved on Linux (fallback behavior), and how lyrics wrap.
- [ ] **Basics.** The app opens; a video plays with sound (proves the FFmpeg DLLs load); outputs go fullscreen on the right displays; closing the operator window exits the app.

## Later (tracked in the roadmap)

- **Media paths.** Services refer to media by absolute path, so a library copied from Linux cannot find its files on Windows. Until then, re-add media on the church PC. Planned: media copied into the library folder and "relink missing media" ([ROADMAP.md](ROADMAP.md) phase 3).
- **Installer name and version.** The Start menu shows "projection-rust" and the version stays 0.1.0. Planned: a product name and a version bump every build (phase 2).
- **Updates.** Installed by hand. Planned: auto-update with confirmation and rollback (phase 11).
- **Code signing.** Decided to skip for now: SmartScreen warns (More info → Run anyway) and Defender occasionally flags unsigned installers.
- **Remote over the network.** The WebSocket server binds `127.0.0.1`, so no firewall prompt appears. Planned: a LAN setting, with the installer adding the firewall rule, pairing codes and mDNS names (phase 10).
