# Roadmap

Grouped by goal, roughly in priority order. Not a schedule. **Open** marks something still to decide. The bar is the church PC: Ryzen 3 2200G, 8 GB, GTX 1050 Ti, Windows 10/11, a 1080p projector and a 720p confidence screen.

Released: `v0.2.0`.

## 1. Trust it on Sunday

Many features have so far only been compile-checked for Windows. Prove them on the real machine before adding more.

- [x] Audio underruns after seeks and "Audio flush not acknowledged": seeks no longer wait on the output.
- [x] Idle memory growth: paused videos without audio read up to 64 MB ahead; now 4 MB.
- [x] System check on the first start of each version (`logs/system-check.log`, or run `PALTO.exe --system-check`): computer, storage, displays, fonts, audio, controllers, capture, keep-awake, network, firewall, GPU effects speed, and the video decoder, chosen from the library's videos.
- [ ] Church-PC visit with one installer carrying every fix. Bring back `system-check.log`. Check by hand: display scaling at 125–150 %, projector hotplug, HDMI audio switching, crash recovery, live monitors (DX12 vs Vulkan), clicker, MIDI and Stream Deck presses, window capture and Desktop Duplication, the phone remote from a phone, Companion, an update and a rollback, the 30-minute soak, a full rehearsal, and that closing leaves no process.
- [ ] Add the updater signing key as the `TAURI_SIGNING_PRIVATE_KEY` CI secret, so tag builds can ship updates.

## 2. One data model for desktop and web

PALTO and the web app now share one model: `protocol::model` on the desktop, the same shape in Convex. The engine plays a resolved copy of the loaded service.

Decided: a library song is a reference that service edits never change; library fixes reach upcoming services but not past ones; string IDs shared with Convex; edits as small operations, the same shape on both sides.

- [x] Shared model: songs with sections, services with arrangements and service-only variations, upcoming and past.
  - A section is a type (Verse, Chorus, Bridge…) with an optional number, name and colour; a blank line splits slides.
  - Songs have versions. An edit changes the current version in place unless a past service points at it, then it starts a new one.
  - A service becomes past the day after its date, or when the operator ends it live; it then pins the song versions it used.
  - A service item may override, for that service only: the arrangement, a section's lyrics or slide breaks, the key, notes, background and shown title.
  - Every item kind syncs (songs, text, media, presentations, captures, headers). File paths and capture sources stay per device; media matches by content hash.
  - Desktop "projects" became services with a date and time, title, leader and notes. Times are stored as UTC and shown in local time.
  - Deleting a song archives it, so services still resolve it and it can be restored.
- [x] Move SQLite to it. Dev data was dropped instead of migrated.
- [x] Undo and redo from a stored edit history, per service and per song. Convex logs the same edit operations.
- [x] Song credits, all optional: authors with a role (words, music, translation), copyright, CCLI number, publisher, year. Filled by OpenLyrics import.
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

- [ ] **OBS lyrics plugin.** A native Rust plugin in its own `obs/` project. It reads a read-only `/feed/v1` WebSocket with no pairing, finds PALTO through mDNS `_palto._tcp` or a subnet probe, and draws with OBS's own text sources. The feed has its own "Share lyrics with OBS" switch. Built and tested on Linux OBS 32; still to check on the church's stream PC: finding PALTO by mDNS and by the probe, the firewall, blanking, and a full service.
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
