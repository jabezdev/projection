# Church PC visit checklist

One page to print and take to the church PC (Windows 10/11, GTX 1050 Ti). Time there is short: bring one installer with every fix batched in. Background for each check is in [WINDOWS.md](../WINDOWS.md).

Date: ____________  Version installed: ____________  Displays connected: ________________________

## Before leaving

- [ ] Build the installer on Linux: `desktop/tools/build-windows.sh` (bumps the version). Copy `PALTO_<version>_x64-setup.exe` to a USB stick.
- [ ] Also copy: a typical worship video with sound, a logo image, `desktop/tools/e2e/` (for the soak and profile) and the Bun installer for Windows (https://bun.sh).

## On the church PC

| ✓ | Check | How | Notes |
|---|---|---|---|
| ☐ | Install and start | Run the installer (SmartScreen: More info → Run anyway). Start PALTO from the Start menu. The Start menu and title bar say **PALTO**. | |
| ☐ | Upgrade in place | If an older version was installed, it is replaced, not installed twice (Apps & features lists one PALTO). | |
| ☐ | GPU | Settings → Troubleshooting → **Open logs folder**. `palto.log` names the adapter ("Native output adapter: NVIDIA GeForce GTX 1050 Ti …"). If outputs stay black, note any error lines. | |
| ☐ | Displays | Screens: assign projector and confidence, **Show**. Each output fills its display exactly, on the right display. | |
| ☐ | Scaling | Set the operator display to 125 % or 150 % (projector at 100 % if possible). Repeat **Displays**. | |
| ☐ | Projector hotplug | Unplug the projector, plug it back in. Then start PALTO with the projector unplugged and plug it in afterwards. The saved assignment finds it again (Screens shows it connected). | |
| ☐ | HDMI audio | Play the video. Unplug and replug the projector during playback: the media card shows "reconnecting", then sound continues on the new default device. | |
| ☐ | Fonts | Settings: try the fonts you plan to use. Lyrics wrap the same as on Linux; a missing font falls back to Arial. | |
| ☐ | Keep awake | Note the PC's screen-off time (Settings → System → Power). Show outputs and leave the PC untouched for longer than that. The displays stay on. Hide outputs: the normal timeout returns. | |
| ☐ | Crash recovery | Load a service, cue a song's second slide. End PALTO in Task Manager. Start it again: the same service is open at the same slide, media paused, outputs hidden, with a notice. | |
| ☐ | Backup | Settings → **Export backup…** writes a zip. The weekly automatic backup folder exists under `%APPDATA%\com.magi.projection\backups`. | |
| ☐ | Effects and animated backgrounds | Settings → Projection: turn on shadow, outline and glow; give a song the Aurora background (song editor). Advance lyrics for a minute with both outputs shown. `palto.log` should keep near 60 fps on both outputs and must not say a shader background "cannot keep up". Note anything that stutters. | |
| ☐ | Keys and controllers | → / ← / B / C / L work with the operator window focused; a presenter clicker advances. If a MIDI controller or Stream Deck is available, bind one button in Settings → Controls and use it. | |
| ☐ | Video profile | For each video you plan to use: `bun tools\e2e\profile.ts --video <file> --compare --app "<PALTO folder>\PALTO.exe"` (about 1 min 20 s each). It plays the video with software and then hardware decoding and writes `profile-*.md`. Keep the reports; they decide the decoder setting and fill in [docs/media-profile.md](media-profile.md). | |
| ☐ | Hardware decoding | Settings → System → Video decoding lists the accelerators found (expect Direct3D 11 and NVIDIA). Choose **Automatic**, then play each video on both outputs: picture correct (no green or corrupted frames), sound in sync, seeking and looping work. If anything looks wrong, set it back to **Software** and note which video. | |
| ☐ | Soak (30 min) | `bun tools\e2e\soak.ts --video <file> --app "<PALTO folder>\PALTO.exe"` (the folder: Start menu → PALTO → Open file location; usually `%LOCALAPPDATA%\PALTO`). Passes; keep the CSV. | |
| ☐ | Full rehearsal | The seven steps in README.md → MVP rehearsal, with the real songs, media and displays. | |
| ☐ | Exit | Closing the operator window closes everything; no PALTO process remains in Task Manager. | |

## Bring back

- `palto.log` (and any `palto.N.log`) from the logs folder, the soak CSV, and photos of anything that looked wrong.
- Write each failure on the back of this page: what you did, what you expected, what happened.
