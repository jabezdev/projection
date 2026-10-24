# Roadmap

Long-term plan for Magi Projection. The current design and its settled decisions are in [ENGINE.md](ENGINE.md) and [ARCHITECTURE.md](ARCHITECTURE.md); nothing here overrides them. Windows release checks for the MVP are tracked in [WINDOWS.md](WINDOWS.md).

**V1** is phases 1–11: everything needed to run a full service, including outside sources and presentations. **[V2](#v2)** collects larger directions for after V1 ships; they are not designed yet.

The phases are in priority order, but they are not a schedule: pick up any item whose dependencies are done. Decisions already made are recorded under each phase; open questions are marked **Open**.

`[x]` done · `[/]` partly done · `[ ]` not started

## Target machine

Everything is judged against the church PC: **AMD Ryzen 3 2200G, 8 GB DDR4-2400, NVIDIA GTX 1050 Ti (4 GB), Windows 10/11**. This is a design target, not a measured minimum. Static lyrics and images fit comfortably; video, capture and effects must be measured on this machine before live use.

Intended display setup (all at once, any resolution, aspect ratio or DPI):

| Display | Example | Purpose |
|---|---|---|
| Operator primary | 1920×1080 | Control panel |
| Main projection | 1920×1080 | Audience |
| Confidence | 1280×720 | Stage |
| Operator secondary | 1920×1080 | Extra workspace and screen-capture source |

Check the 1050 Ti's ports and simultaneous display limit. Using the 2200G's integrated output as well would put displays on two GPUs, which the engine does not support yet.

## Where things stand (MVP, 2026-10-11)

- [x] Native projection and confidence outputs on one GPU device; no webview on either. Runs on Omarchy Linux and Windows 10.
- [x] Lyrics with crossfades and a persistent border card; images, image presentations, foreground video with audio, standalone audio, muted looping song/Logo video backgrounds.
- [x] Live, Clear, Black and Logo with the agreed media policy.
- [x] Confidence teleprompter: clock, arrangement tape, sticky part labels, smooth scrolling, optional shared video.
- [x] One decoder per video for both screens, one audio output with device reconnect, timestamp-driven playback, seek, loop, clip bounds, speed.
- [x] Operator: three resizable columns, song and service editing, local SQLite library, media cards with transport, native preview stills, output settings, display assignment and fullscreen per output.
- [x] WebSocket remote control (same machine), typed protocol checked against the UI at build time.
- [x] Song library web app: Convex backend, Library, Services, Team roles, pairing codes.

---

## 1. Ready for a live service

Goal: nothing at church can fail silently or without a way back.

- [x] **Log file** in `%APPDATA%\com.magi.projection\logs\` (Linux: the app-data folder), rotated by size (5 × 5 MB).
- [x] **Keep the display awake** while any output is shown (`SetThreadExecutionState` on Windows; an idle inhibitor on Linux); release it when outputs are hidden.
- [x] **"Open logs folder"** button in Settings.
- [x] **Crash recovery:** on the next start after a crash, reopen the last service at the last cue. Outputs stay hidden until the operator shows them.
- [x] **Church-PC visit checklist:** one printable page covering scaling, projector hotplug, HDMI audio, GPU and fonts (from [WINDOWS.md](WINDOWS.md)): [docs/church-pc-checklist.md](docs/church-pc-checklist.md).
- [x] **Soak script:** cue a video with confidence on for 30 minutes, log memory, decoders and frame counts, and fail if anything grows: `bun desktop/tools/e2e/soak.ts --video <file>`.
- [ ] Finish the remaining [WINDOWS.md](WINDOWS.md) checks and a full rehearsal on the church PC ([README.md](README.md#mvp-rehearsal)).

## 2. Foundations

Goal: make the big changes ahead safe. Done right after phase 1.

- [x] **CI on GitHub Actions:** `cargo test --workspace`, the frontend build/lint/test and the web typecheck/test on every push; Windows cross-build on tags.
- [x] **Split `main.rs`** (about 850 lines) into commands, windows/displays and settings modules.
- [x] **Split `App.tsx`** into smaller components, ahead of the library and settings revamps.
- [x] **End-to-end WebSocket test script** in the repo (cue, transport, display modes, save/load), runnable against a scratch data folder: `bun desktop/tools/e2e/e2e.ts`.
- [x] **Extend the parity harness** to Black, Logo and video scenes (native references, since the webview had none worth keeping).
- [x] **Naming and versioning:** the product is **PALTO** (installer, Start menu, windows); versions are `0.MINOR.BUILD`, with the build number bumped by every `build-windows.sh` run so installers upgrade in place.
- [x] **Backup and export:** a one-click export of `app.db`, settings and the media list to a zip, plus an automatic weekly backup (keep the last few). Needed before the data model changes in phase 3.

## 3. Library revamp and data model

Goal: a library that is the clean reference for every song, with services built on top of it.

Decisions:
- **The library song is a reference artifact.** Editing a service (arrangement, variations) never changes the library song.
- **Library fixes reach upcoming services, not past ones.** A typo fixed in the library shows up in services that have not happened yet; past services stay exactly as they were sung. Per-service variations are never overwritten.
- **String IDs everywhere** (desktop and cloud), so local records and Convex records share identity. Nothing has been released, so existing data is migrated once.
- **Media is copied into a library folder on import**, so the library is self-contained and can be backed up and synced.
- **Undo is backed by a full edit history in SQLite.** Every edit is stored as a small operation (the same shape the web app sends to Convex). The same log later feeds sync (phase 4).

Items:
- [ ] Rework the data model (desktop and Convex together): songs and sections as the reference; services with arrangements and variations; "upcoming vs past" service state. **Open:** details to be designed first.
- [ ] Migrate the SQLite schema and existing libraries to string IDs and the new model, with the phase 2 backup taken first.
- [ ] Edit history in SQLite; undo/redo for library and service edits.
- [ ] Library UI revamp: browse, filter and edit songs and media in one place.
- [x] Quick search (Ctrl+K): find a song or media file and add it to the service.
- [/] Copy media into the library folder on import; move existing referenced files in during migration. *Import, presentation images and the logo copy into `media/` (identical files reused); moving existing files waits for the migration.*
- [x] Relink missing media: search a chosen folder by file name (Library → Missing).
- [x] Import songs from OpenLyrics XML (Library → Songs → Import OpenLyrics…).
- [ ] **Service templates** designed by the operator: any number of templates (e.g. "Sunday", "Youth night"), each a reusable structure of placeholder slots and fixed items; create a service from a template and edit templates later.

## 4. Cloud sync (offline-first)

Goal: the team plans on the web during the week; the projection computer always runs from its own copy and syncs when it can.

Decisions:
- **Offline-first.** The desktop keeps a full local copy of the library, services and media. Nothing on the live path waits for the network.
- **Full offline mode:** a setting that stops all syncing. The web team keeps working; changes meet when the computer reconnects.
- **The operator owns a service while it is live.** The web shows it as "live on site"; edits made there during the service are held and arrive after it ends. Other services stay editable on the web.
- **Two-way edits.** Desktop edits are sent as the same small operations the web uses, queued while offline (from the phase 3 edit history).
- **Conflicts never block the operator.** Changes that touched the same thing on both sides go into a "needs attention" list; the operator settles each in a side-by-side dialog when they choose. Everything that does not conflict merges automatically.
- **Sync switches (flags)** for costly or chatty updates such as "now showing". An admin sets a team default on the web and can override it per projection computer; the computer uses its last known settings while offline.
- **Media syncs to Convex storage**, local first: files up to 100 MB sync both ways; larger files stay local. Media is identified by content (hash), so the same file is never uploaded twice; a file changed on both sides is a conflict like any other.
- **Web hosting on Cloudflare Pages.**

Items:
- [ ] Deploy the Convex production backend and host `web/app` on Cloudflare Pages.
- [ ] Pairing screen on the desktop; store the device credential.
- [ ] Full offline mode setting.
- [ ] Pull services and songs into SQLite (library songs kept as local reference copies); background refresh while online; "last synced" indicator.
- [ ] Push queued desktop operations; resume after reconnect.
- [ ] "Live on site" ownership: mark a service live, hold web edits for it, release them afterwards.
- [ ] "Needs attention" list and the side-by-side conflict dialog.
- [ ] Sync switches: team defaults and per-computer overrides, edited on the web, cached on the desktop.
- [ ] "Now showing" behind a switch, rate-limited; singers' view highlights the current part. Ephemeral updates are never sent unless their switch is on.
- [ ] Media sync to Convex storage: 100 MB cap, hashing, upload/download queue that never competes with live playback, conflict handling.

## 5. Looks and settings revamp

Goal: each screen looks right on its own, with effects that stay within the target machine's budget.

Decisions:
- **Each screen has its own parameters** (text size, margins, placement, effects, transitions); the settings page is redesigned around screens.
- **Text shadow** first, as a blurred glyph pass; **outline (and glow)** through signed-distance-field glyphs. *(2026-10-24: the shadow uses the same distance-field glyphs instead of a separate pass; see ENGINE.md decision 10.)*
- **Transitions:** crossfade, cut and slide, each with parameters (duration, easing, direction).
- **Shader backgrounds** are supported, with built-in presets and hard limits (frame rate cap, render resolution cap, automatic fallback to a still) so they cannot take over the GPU.
- **One icon set** for the control panel and drawable on the outputs. Color emoji are not planned.

Items:
- [x] Settings page revamp: per-screen sections, live preview per screen.
- [x] Per-screen parameters for projection and confidence (beyond today's confidence text size).
- [x] Shape whole lines (fixes kerning across spaces). *Done as kerning restored around spaces after word shaping, left-to-right lines.*
- [x] Prepare the next and previous cues ahead of time (images, a video's first frame, the logo video).
- [/] Text shadow, cost-checked on the 1050 Ti. *Built from distance-field glyphs rather than a separate blur pass (see ENGINE.md decision 10); not yet measured on the 1050 Ti.*
- [x] Outline and glow (SDF glyphs).
- [x] Transition styles and their parameters.
- [x] Shader backgrounds: presets, resource limits, static fallback. *Gradient, aurora and bokeh; not yet measured on the 1050 Ti.*
- [x] Icon set in the operator UI and an icon element on the outputs. *Lucide (decided 2026-10-24): `lucide-react` in the operator UI; on the outputs `:name:` in slide text (e.g. `:heart:`, `:book-open:`) draws that icon from the bundled Lucide font, in the text's color and effects.*

## 6. Operator controls

Goal: run a whole service without the mouse, from any device.

Decisions:
- **Configurable bindings** from the start, on one input layer that keyboards and outside devices share.
- **Devices, long term:** presenter clickers, MIDI controllers, Stream Deck, and network control (OSC / Bitfocus Companion).
- **Confirm risky actions** (deleting an item, loading another service) only while outputs are shown; undo covers the rest.

Items:
- [x] Input layer and bindings editor; default keyboard map (→/Space next, ← previous, Black/Clear/Logo/Live keys).
- [x] Presenter clickers (PageUp/PageDown).
- [x] Pending vs on-air indicator: what was selected vs what the output actually shows while media loads.
- [x] Confirmations while outputs are shown (loading another service, removing an item).
- [x] MIDI input. *Not yet tried with a physical controller.*
- [x] Stream Deck (keys only; no key images yet). *Not yet tried with a physical deck.*
- [x] OSC / Companion over the network (uses phase 10's network access and pairing). *`/palto/<action>` on UDP 9000 through the input layer; other computers need LAN access and an allowed-list entry, since OSC cannot pair. Not yet tried with Companion.*
- [ ] **Stage tools on the confidence screen:** timers, operator text messages and similar. **Open:** design when we get there.

## 7. Media and performance

Goal: smooth video on the target machine with room to spare. Profile first; optimize what the numbers show.

Decisions:
- **Hardware decoding is selectable**, with automatic detection and a setting to pick or disable an accelerator (in case of driver problems). Candidates: Windows D3D11VA, DXVA2, NVDEC, Intel QSV, AMD AMF; Linux VA-API, VDPAU, NVDEC, Vulkan Video. Software decoding always remains as the fallback.

Items:
- [/] Profiling harness: decode, convert, upload and render times, logged; run on the church PC. *A `profile:` log line every 5 s and `bun tools/e2e/profile.ts --video <file> --compare`, which writes a report; not yet run on the church PC.*
- [x] Upload YUV planes and convert in a shader (halves upload size and CPU conversion). *8-bit 4:2:0 up to 4K goes to the GPU as decoded and is scaled there; colors now follow the file's matrix (BT.709 for HD).*
- [x] Detect available hardware decoders per platform; setting to choose one or turn it off; fall back to software on failure. *Settings → System → Video decoding; Software stays the default until checked on the church PC. QSV and AMF are reached through D3D11VA/VA-API.*
- [x] Seamless background loops (prepare the loop point instead of seeking at the end). *The clip's first frames are kept and queued again at each wrap.*
- [/] Supported media profile: tested codecs, resolutions and bitrates, documented: [docs/media-profile.md](docs/media-profile.md). *Measured on the development laptop; the church PC table is still empty.*
- [ ] Displays on two GPUs (1050 Ti plus the 2200G's output), only if the display setup needs it.

## 8. Screen capture

Goal: show another window or screen on the outputs, from this computer or from another device on the local network.

Decisions:
- **Capture a window or a whole screen.**
- **Best API per platform, switchable:** the app detects what is available, picks a default, and lets the operator switch if the default misbehaves. Windows: Windows.Graphics.Capture (default), Desktop Duplication (fallback). Linux: PipeWire screencast portal under Wayland; XComposite/XShm under X11.
- **Routing is reconfigurable:** projection, confidence or both, with fit/fill per screen.
- **Capture once, show on both;** handle disconnects and size changes with a visible fallback, never a frozen image on air.
- **Network capture needs nothing installed on the sending device.** The app hosts a small screen-share web page on the LAN. A presenter opens it in a browser (found through mDNS, phase 10), picks what to share, and the feed arrives as a capture source like any other.
- **Share a tab, a window or a whole screen**, using the browser's own picker.
- **Sender video settings:** resolution (e.g. 720p, 1080p, source) and bitrate, chosen on the share page, with defaults set by the operator.
- **Whole-desktop audio, not only tab audio.** Shared audio plays through the app's one audio output (ENGINE.md decision 2).

Items:
- [ ] Capture backend detection and selection setting.
- [ ] Window and screen pickers.
- [ ] Capture as an output source with per-screen routing and fit/fill.
- [ ] Shared capture frame for both screens; resolution changes; disconnect fallback.
- [ ] Screen-share web page served by the app: tab, window and screen sharing; resolution and bitrate settings; connection status for the presenter.
- [ ] Receive the feed in the app (WebRTC) and decode it into a capture source; one sender at a time, or a list the operator picks from. **Open:** one or several senders.
- [ ] Whole-desktop audio from the sending device, mixed into the single audio output with its own volume.
- [ ] Latency and load measured on the target machine at 1080p.
- [ ] **Open:** browsers only allow screen sharing on secure (HTTPS) pages, so the share page needs a certificate the sending device trusts. Options: a self-signed certificate (one-time browser warning), or a domain with a real certificate that points at the LAN address.
- [ ] **Open:** browsers limit whole-desktop audio (Chromium offers it when sharing a whole screen on Windows; other browsers and systems vary). Check what the church's devices support before promising it, and say plainly on the share page when audio is not available.

## 9. Presentations and web pages

Goal: show the presentations people actually bring, including browser-based ones, without leaving the app.

Decisions:
- **Browser embeds**, especially the Canva presentation player, and other browser-based presentations (e.g. Google Slides). The operator steps through them from the app (next/previous reach the page).
- **The outputs stay native** (ENGINE.md decision 1). A web page is drawn off screen and shown as a source, the same way as a capture; it never turns an output back into a webview.
- **Presentation files:** PDF, PowerPoint (PPT/PPTX) and other common formats (e.g. Keynote, OpenDocument), imported as slides the operator steps through like an image presentation.

Items:
- [ ] Web page source: open a URL, render it off screen, show it on the outputs with per-screen routing and fit/fill; forward next/previous and clicks from the operator. **Open:** how to render off screen (an embedded browser engine such as CEF, or a hidden WebView2/WebKit window captured through phase 8), judged by cost on the target machine.
- [ ] Canva presentation player tested end to end (sign-in, fullscreen player, stepping).
- [ ] Other browser presentations: Google Slides and similar, tested one by one.
- [x] PDF import: render each page to a slide at output resolution. *pdf.js in the operator window renders each page to a 1920×1080 PNG in the media folder; no native PDF library to ship.*
- [ ] PowerPoint and other formats. **Open:** convert to slides through an installed office suite (PowerPoint or LibreOffice) or require a PDF export; slide animations and embedded video are probably lost either way.
- [ ] Speaker notes and the next slide on the confidence screen, where the format provides them.

## 10. Remote control

Decisions:
- **LAN access is a setting** (off by default); the installer adds the Windows Firewall rule.
- **Remotes pair with a code** shown on the operator screen.
- **mDNS for every local web app the computer hosts** (remote control, screen share), so devices reach them by name (e.g. `projection.local`) instead of an IP address. The operator screen also shows the address and a QR code, for devices that do not resolve `.local` names.
- **Phone remote as a PWA** served by the app for now; a native app is decided later (Tauri mobile is the likely option and would re-add the `[lib]` crate type and a `lib.rs` entry point to `desktop/crates/app`).

Items:
- [x] LAN toggle with configurable port; firewall rule in the installer. *Settings → Remote; the NSIS hook adds a Private/Domain rule once, with an administrator prompt. Not yet tried on Windows.*
- [/] mDNS advertising for the remote and the screen-share page; address and QR code on the operator screen. *`palto.local` (decided 2026-10-24) for the remote, with addresses and a QR code in Settings → Remote; the screen-share page waits for phase 8. Renaming is not offered.*
- [x] Pairing-code authentication for remote clients. *Six-digit code for 5 minutes, token per device, removable; web pages from other sites on this computer are refused too.*
- [x] Phone remote PWA: next/previous, display modes, current and next slide. *Minimal design, served by the app. Browsers allow installing it only over HTTPS, so on plain HTTP it is added to the home screen as a shortcut.*
- [x] Throttle frequent remote commands. *15 a second per connection, bursts of 30.*
- [ ] Native phone app. **Open:** decide the approach.

## 11. Release and updates

Decisions:
- **Auto-update** through the Tauri updater and GitHub Releases: downloads in the background, asks before installing, never installs during a live service. Keep the previous installer so the operator can roll back.
- **No code signing** for now (SmartScreen will warn).

Items:
- [ ] Updater with background download, install confirmation and a "roll back to previous version" option.
- [ ] Release notes shown before installing.
- [ ] macOS: a separate validation pass before claiming support.
- [ ] Native Wayland on Linux (a Wayland subsurface for the outputs); XWayland works today.

## V2

Larger directions for after V1. None of these is designed yet; each starts with a design pass and its own decisions, and each is judged again against the target machine (which may need to change for some of them).

- [ ] **Sync with our other software.** Connect this app with the other tools we will build, so they share state and react to each other. **Open:** which apps, what they share, and the protocol (likely built on the phase 3 edit operations and the phase 6 input layer).
- [ ] **Motion-graphics text.** Text styled and animated at After Effects level: per-letter and per-line animation, keyframes, masks, layered effects.
- [ ] **2D and 3D scenes.** A game-engine-like layer for animated 2D and 3D elements on the outputs, combined with text and media.
- [ ] **Parameterized backgrounds with external triggers.** Backgrounds expose parameters (color, speed, intensity and so on) that change live from triggers: the current song part, MIDI, OSC, audio level, other software. Builds on phase 5's shader backgrounds and phase 6's input layer.

## Not planned

Color emoji, HDR, genlock (hardware-synchronized scanout across monitors), multi-machine output sync and streaming the outputs over the network are out of scope unless a real need appears. Receiving a screen feed from another device is planned (phase 8). A general timeline editor is not planned for V1; keyframed animation is part of V2's motion-graphics text.
