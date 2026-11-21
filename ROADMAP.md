# Roadmap

Phases are in priority order but are not a schedule: pick up any item whose dependencies are done. **Open** marks a question still to decide. The settled design is in [ARCHITECTURE.md](ARCHITECTURE.md).

Everything is judged against the church PC: **Ryzen 3 2200G, 8 GB, GTX 1050 Ti, Windows 10/11**, driving an operator screen, the projector (1080p) and a confidence screen (720p), maybe a second operator screen.

## Status (2026-11-21)

Released: `v0.2.0` (the redesigned operator workspace). Done: phases 1 and 2 (live readiness, CI, backups), 5 (per-screen looks, effects, transitions, shader backgrounds, Lucide icons), 6 (input layer, MIDI, Stream Deck, OSC), 7 (YUV upload, hardware decoding, seamless loops, profiling, live monitors), 10 (LAN remote, pairing, `palto.local`, phone remote), 11 (updater), local screen capture and PDF import. The codebase was reorganized into small modules and moved to FFmpeg 9 the same day.

## Working order (2026-11-21)

- [ ] **1. Data model:** songs, sections and services with variations; upcoming vs past; string IDs; edit history and undo; song credits; service templates (phase 3).
- [ ] **2. Visual revamp:** the operator UI's look. **Open:** design.
- [ ] **3. UI, usability and display nitpicks:** collected as found.
- [ ] **4. Performance and optimizations:** audio underruns after seeks and "Audio flush not acknowledged"; baseline memory 298 → 366 MB since phase 5; church-PC profile.
- [ ] **5. Stage tools:** timers and operator messages on the confidence screen (phase 6).
- [ ] **6. Screen sharing from other devices:** share page, WebRTC, audio (phase 8).
- [ ] **7. PowerPoint import** (phase 9).
- [ ] **8. OBS plugin:** designed; waits on item 1 for credits (phase 12).
- [ ] **9. Web app:** hosting, Convex production, then sync with PALTO (phase 4).
- [ ] **10. Others:** native phone app, macOS, native Wayland, the updater signing key as a CI secret.

## Next church-PC visit

Bring one installer with every fix batched in, a test video, the logo and `desktop/tools/e2e/` with Bun.

Already seen in `palto.log` (1050 Ti, Vulkan): the GPU is found; outputs hold 60 fps; video plays with sound; screen capture starts; displays show by model name. Found there: **audio underruns after seeks and repeated "Audio flush not acknowledged" warnings**. Fix these before the visit.

Still to check: display scaling at 125–150 %; projector hotplug (Windows renumbers displays); HDMI audio switching during playback; fonts and wrapping; keep-awake; crash recovery; backup export; live monitors (layering, scaling, `WGPU_BACKEND=dx12` vs Vulkan); effects and Aurora at 60 fps; keys, clicker, MIDI and Stream Deck; `profile.ts --compare` per video (fills the media profile and decides the decoder default); hardware decoding picture and sync; phone remote and the firewall rule; Companion; capture of a window and Desktop Duplication; an update and roll-back; the 30-minute soak; a full rehearsal; closing leaves no process. Bring back the logs, the soak CSV and notes.

## 3. Library revamp and data model

Decided: the library song is a reference and service edits never change it; library fixes reach upcoming services, not past ones; string IDs shared with the cloud; media copied into the library; undo from a full edit history in SQLite, in the same operation shape the web sends to Convex.

- [ ] Data model for desktop and Convex together: songs and sections, services with arrangements and variations, upcoming vs past. **Open:** the details.
- [ ] Migrate SQLite to string IDs and the new model (after a backup); move existing media into `media/`.
- [ ] Edit history; undo/redo for library and service edits.
- [ ] Library UI revamp: browse, filter and edit songs and media in one place.
- [ ] Song credits (authors, copyright, CCLI number), edited on desktop and web, filled by OpenLyrics import. Needed by phase 12.
- [ ] Service templates designed by the operator ("Sunday", "Youth night"): placeholder slots and fixed items.

## 4. Cloud sync (offline-first)

Decided: full local copy, nothing live waits on the network; a full-offline setting; the operator owns a live service and web edits to it wait until it ends; two-way edits as small operations; conflicts go to a "needs attention" list with a side-by-side dialog and never block; sync switches with team defaults and per-computer overrides; media up to 100 MB syncs by content hash; web hosted on Cloudflare Pages.

- [ ] Deploy Convex production and host `web/app`.
- [ ] Pairing screen on the desktop; full offline setting.
- [ ] Pull services and songs; background refresh; "last synced".
- [ ] Push queued operations; resume after reconnect.
- [ ] "Live on site" ownership; the conflict list and dialog.
- [ ] Sync switches; "now showing" for the singers' view behind one.
- [ ] Media sync with an upload queue that never competes with playback.

## 6. Operator controls

- [ ] Stage tools on the confidence screen: timers, operator messages. **Open:** design.

## 7. Media and performance

- [ ] Profile on the church PC and fill in the media profile.
- [ ] Displays on two GPUs, only if the setup needs it.

## 8. Screen capture from other devices

Decided: nothing to install on the sender; PALTO hosts a share page on the LAN (found through mDNS); tab, window or screen through the browser's picker; resolution and bitrate chosen there; whole-desktop audio through the one audio output.

- [ ] Share page served by PALTO with connection status.
- [ ] Receive the WebRTC feed as a capture source. **Open:** one sender or a list.
- [ ] Sender audio with its own volume; latency and load measured at 1080p.
- [ ] **Open:** HTTPS for the share page (self-signed with a one-time warning, or a real domain pointing at the LAN address).
- [ ] **Open:** which browsers on the church's devices can share desktop audio.

## 9. Presentations and web pages

Decided: browser presentations (Canva, Google Slides) are drawn off screen and shown as a source, so outputs stay native; presentation files become slides.

- [ ] Web page source with next/previous and clicks forwarded. **Open:** CEF or a hidden WebView2/WebKit window captured, judged by cost.
- [ ] Canva player end to end; Google Slides and similar.
- [ ] PowerPoint and other formats. **Open:** convert through an installed office suite or require a PDF export.
- [ ] Speaker notes and the next slide on the confidence screen.

## 10–11. Remote and releases

- [ ] Native phone app. **Open:** approach (Tauri mobile likely).
- [ ] macOS validation; native Wayland outputs (a Wayland subsurface).

## 12. OBS lyrics plugin

Goal: the livestream computer shows the on-air lyrics and song credits in OBS, with nothing to type and no pairing. Full design: [docs/obs-plugin.md](docs/obs-plugin.md). Decided 2026-11-21; waits for the phase 3 data model so credits are in the first release.

Decisions:
- **Native OBS plugin in Rust** (a `cdylib` over a thin libobs FFI), not a Browser Source page, to keep the stream PC light. It lives in a new independent `obs/` project with its own releases (`obs-v*` tags).
- **A read-only lyrics feed, open on the LAN, with no pairing.** Commands still need pairing. The feed is its own stable, versioned contract (`/feed/v1`, additive only), not the engine's events.
- **The plugin finds PALTO by scanning:** mDNS (`_palto._tcp`) first, then a subnet probe of `/api/check`. A manual address is the fallback.
- **Text is drawn by OBS's own text source** inside the plugin's sources, so styling is OBS's usual text settings.
- **Windows x64, OBS 30 or newer**; Linux for development. NSIS installer and zip on GitHub Releases; no auto-update; unsigned.

Items:
- [ ] Feed contract `protocol/src/feed.rs` with shared fixtures; `/feed/v1` WebSocket; `feed_versions` and an app id in `/api/check`.
- [ ] `_palto._tcp` mDNS advertisement with name, version and feed version.
- [ ] Feed reachability setting and connected feed clients in Settings → Remote. **Open:** follow the LAN access switch, or a separate "Share lyrics with OBS" switch (recommended).
- [ ] `obs/` skeleton: loads in OBS, registers a source, shows text through a child text source.
- [ ] Feed client with reconnect, blanking on Clear/Black/Logo, and status; discovery list in the source properties.
- [ ] Sources: PALTO Lyrics, PALTO Song Info (credits template), PALTO Section; fades.
- [ ] Windows cross-build, installer, CI job, first `obs-v0.1.0` release after a church check (stream PC, firewall, mDNS, soak).

## V2 (not designed)

Sync with our other software; After-Effects-level motion text; 2D and 3D scenes; backgrounds with parameters driven live by song parts, MIDI, OSC or audio.

**Not planned:** color emoji, HDR, genlock, multi-machine output sync, streaming the outputs, a V1 timeline editor. Code signing is skipped for now (SmartScreen warns).
