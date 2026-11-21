# Roadmap

Grouped by goal, roughly in priority order. Not a schedule. **Open** marks something still to decide. The bar is the church PC: Ryzen 3 2200G, 8 GB, GTX 1050 Ti, Windows 10/11, a 1080p projector and a 720p confidence screen.

Released: `v0.2.0`.

## 1. Trust it on Sunday

Many features have so far only been compile-checked for Windows. Prove them on the real machine before adding more.

- [ ] Fix audio underruns after seeks and the repeated "Audio flush not acknowledged" warnings.
- [ ] Find why idle memory grew from 298 to 366 MB.
- [ ] Church-PC visit with one installer carrying every fix. Check: display scaling at 125–150 %, projector hotplug, HDMI audio switching, fonts, keep-awake, crash recovery, backups, live monitors (DX12 vs Vulkan), effects at 60 fps, clicker, MIDI and Stream Deck, window capture and Desktop Duplication, phone remote and the firewall rule, Companion, an update and a rollback, the 30-minute soak, a full rehearsal, and that closing leaves no process.
- [ ] Choose the default decoder (software or hardware) from `profile.ts --compare` on that PC.
- [ ] Add the updater signing key as the `TAURI_SIGNING_PRIVATE_KEY` CI secret, so tag builds can ship updates.

## 2. One data model for desktop and web

PALTO stores a song as a title plus one lyrics string with numeric IDs. The web app has sections, arrangements and credits. Sync, undo, credits and the OBS plugin all depend on closing this gap.

Decided: a library song is a reference that service edits never change; library fixes reach upcoming services but not past ones; string IDs shared with Convex; edits as small operations, the same shape on both sides.

- [ ] Shared model: songs with sections, services with arrangements and service-only variations, upcoming and past. **Open:** the details.
- [ ] Migrate SQLite to it, after an automatic backup.
- [ ] Undo and redo from a stored edit history.
- [ ] Song credits (authors, copyright, CCLI number), filled by OpenLyrics import.
- [ ] Service templates ("Sunday", "Youth night") with fixed items and empty slots.

## 3. Cloud sync, offline-first

Decided: PALTO keeps a full local copy and nothing live waits on the network; while a service is live, the operator owns it; conflicts go to a "needs attention" list and never block; there is a full-offline setting; media up to 100 MB syncs by content hash.

- [ ] Host the web app (Cloudflare Pages) and deploy Convex production.
- [ ] Pair PALTO with the team account (`devices.ts` already issues codes).
- [ ] Pull songs and services, push queued edits, resume after reconnect, show "last synced".
- [ ] Live ownership, the conflict list and a side-by-side dialog.
- [ ] Media sync through an upload queue that never competes with playback.
- [ ] A "now showing" view for singers.

## 4. The operator's experience

- [ ] Visual redesign of the control panel. **Open:** the design.
- [ ] A library view to browse, filter and edit songs and media in one place.
- [ ] Stage tools on the confidence screen: timers and messages from the operator. **Open:** the design.
- [ ] Small UI and display fixes, collected as they are found.

## 5. More sources and outputs

- [ ] **OBS lyrics plugin.** A native Rust plugin in its own `obs/` project. It reads a read-only `/feed/v1` WebSocket with no pairing, finds PALTO through mDNS `_palto._tcp` or a subnet probe, and draws with OBS's own text sources. Waits on song credits (2). **Open:** whether the feed follows the LAN access switch or gets its own switch.
- [ ] **Screen sharing from other devices.** PALTO hosts a share page on the LAN and receives WebRTC, with audio. **Open:** HTTPS, one sender or several, which browsers can share audio.
- [ ] **Presentations.** PowerPoint import. **Open:** convert through an installed office suite, or require a PDF export. Speaker notes and the next slide on the confidence screen.
- [ ] **Web pages** (Canva, Google Slides) drawn off-screen as a source. **Open:** CEF or a hidden webview, chosen by cost.

## 6. Platforms

- [ ] Native phone app. **Open:** the approach (probably Tauri mobile).
- [ ] macOS.
- [ ] Native Wayland outputs.
- [ ] Displays on two GPUs, only if a venue needs it.

## Later (V2, not designed)

Sync with our other software, motion text at After Effects level, 2D and 3D scenes, backgrounds driven live by song parts, MIDI, OSC or audio.

## Not planned

Color emoji, HDR, genlock, synced output across several machines, streaming the outputs, a timeline editor, code signing for now.
