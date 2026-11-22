# PALTO for OBS

Shows what PALTO has on screen as OBS text: **PALTO Lyrics**, **PALTO Song Info** (title and credits) and **PALTO Section** (for example "Verse 1"). OBS draws the text, so font, colour, outline and size are set in OBS.

1. In PALTO, turn on Settings → Remote → Share lyrics with OBS.
2. On the OBS computer, run `palto-obs-…-setup.exe`, or unzip `palto-obs` into `C:\ProgramData\obs-studio\plugins\`.
3. In OBS, add a PALTO source. It finds PALTO on the network; pick a computer or type its address if needed.

Needs OBS 30 or newer, Windows x64. Plugin 0.1 reads feed v1, from PALTO releases after 0.2.0.

Build: `tools/build-windows.sh` (cross-builds on Linux). Tests: `cargo test`. Releases are tagged `obs-v*`.
