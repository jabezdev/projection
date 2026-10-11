# Roadmap

Long-term plan for Magi Projection. The current design and its settled decisions are in [ENGINE.md](ENGINE.md) and [ARCHITECTURE.md](ARCHITECTURE.md); nothing here overrides them. Windows release checks for the MVP are tracked in [WINDOWS.md](WINDOWS.md).

`[x]` done · `[/]` partly done · `[ ]` not started

## Target machine

Everything below is judged against the church PC: **AMD Ryzen 3 2200G, 8 GB DDR4-2400, NVIDIA GTX 1050 Ti (4 GB), Windows 10/11**. This is a design target, not a measured minimum. Static lyrics and images fit comfortably; video, capture and effects must be measured on this machine before live use.

Intended display setup (all at once, any resolution, aspect ratio or DPI):

| Display | Example | Purpose |
|---|---|---|
| Operator primary | 1920×1080 | Control panel |
| Main projection | 1920×1080 | Audience |
| Confidence | 1280×720 | Stage |
| Operator secondary | 1920×1080 | Extra workspace and screen-capture source |

Check the 1050 Ti's ports and simultaneous display limit. Using the 2200G's integrated output as well would put displays on two GPUs, which the engine does not support yet (see below).

## Where things stand (MVP, 2026-10-11)

- [x] Native projection and confidence outputs on one GPU device; no webview on either. Runs on Omarchy Linux and Windows 10.
- [x] Lyrics with crossfades and a persistent border card; images, image presentations, foreground video with audio, standalone audio, muted looping song/Logo video backgrounds.
- [x] Live, Clear, Black and Logo with the agreed media policy.
- [x] Confidence teleprompter: clock, arrangement tape, sticky part labels, smooth scrolling, optional shared video.
- [x] One decoder per video for both screens, one audio output with device reconnect, timestamp-driven playback, seek, loop, clip bounds, speed.
- [x] Operator: three resizable columns, song and service editing, local SQLite library, media cards with transport, native preview stills, output settings, display assignment and fullscreen per output, saved across restarts.
- [x] WebSocket remote control (same machine), typed protocol checked against the UI at build time.
- [x] Song library web app: Convex backend, Library, Services, Team roles, pairing codes.

## 1. Ready for a live service

- [ ] Finish the [WINDOWS.md](WINDOWS.md) list: log file, keep the display awake, scaling, projector hotplug, HDMI audio, fonts.
- [ ] Full rehearsal on the church PC with every display connected (steps in [README.md](README.md#mvp-rehearsal)).
- [ ] 30-minute soak: continuous video with confidence on; no flicker, black flashes, duplicate audio, growing memory or drift.
- [ ] Verify a packaged Linux build (AppImage/.deb) end to end.
- [ ] Unplug a real audio device during playback (recovery is only unit-tested).
- [ ] Measure: confidence relayout time after an edit on a long service; click-to-first-frame for a lyric cue (target p95 under 50 ms); operator resize/scroll while video plays.

## 2. Cloud sync (offline-first)

The design is in [ARCHITECTURE.md](ARCHITECTURE.md#sync-planned). The desktop always runs from its local library; sync only feeds and updates it.

- [ ] Deploy: Convex production backend, host `web/app` (Vercel/Netlify/Cloudflare Pages).
- [ ] `protocol`: string IDs that can hold Convex IDs; songs keep section labels; active position tracked by sequence entry key.
- [ ] Desktop: pairing screen, service picker, pull services and songs into SQLite.
- [ ] Desktop edits pushed as the same small Convex operations, queued while offline.
- [ ] "Now showing" pushed to Convex; the singers' view highlights the current part.
- [ ] Nice to have: drag-and-drop arranging on the web, chords/keys for singers.

## 3. Media and library

- [ ] Relink missing media (services refer to files by absolute path, so a library moved between machines loses its files).
- [ ] Decide how media travels: keep referencing by path, copy into a library folder, or store in Convex for sync.
- [ ] Prepare the next cue ahead of time (next image, next video's first frame, Logo video) so it appears without a decode delay.
- [ ] PowerPoint/PDF import (converted to image presentations).
- [ ] Seamless video loops (prepare the loop point instead of seeking at end of file).

## 4. Text and visuals

- [ ] **Outline, shadow and glow** for lyrics. Removed from the settings in 2026-10 because the native renderer did not draw them; values saved by older versions are kept in `output-settings.json`. Needs extra render passes (e.g. a distance-field or blurred glyph pass) and a cost check on the 1050 Ti.
- [ ] Kerning across spaces (shape whole lines instead of word by word).
- [ ] Color emoji.
- [ ] Per-output text size, margins and layout for different screen sizes and viewing distances (confidence text size exists; extend to everything).
- [ ] Fit/fill per background with adjustable crop alignment; never stretch unless chosen.
- [ ] Light animated backgrounds: gradients, subtle waves or noise in one fullscreen shader pass. Time-based so dropped frames do not change speed, redraw only while animating, a 30 fps / lower-resolution mode and a static fallback.

## 5. Screen capture

- [ ] Capture the operator's secondary display (up to 1920×1080 at 30 fps, configurable budget) and route it to projection, confidence or both.
- [ ] Capture once and share the frame with both outputs; scale on the GPU to each output's size and fit/fill mode.
- [ ] Short buffering (latest frame wins); handle source resolution changes and disconnects with a visible fallback, never a stale image on air.
- [ ] Use the OS-native API (Windows first: Desktop Duplication / Windows.Graphics.Capture); CPU-copy fallback.

## 6. Performance

Only after profiling on the target machine shows the need.

- [ ] Hardware video decoding with a software fallback; share decoded surfaces with the renderer where possible.
- [ ] Upload YUV planes and convert in a shader, halving upload size and CPU conversion.
- [ ] Reusable staging buffers if uploads show up in profiles.
- [ ] Displays on two GPUs (e.g. 1050 Ti plus the 2200G's integrated output): per-device uploads from one decoder and clock.
- [ ] A tested media profile: start with 1080p30 H.264 at typical bitrates; test other codecs separately.

Record for each test run: process RAM, VRAM, CPU and GPU use, decode and render time, dropped frames, capture latency. Hardware upgrades (16 GB RAM, a faster CPU) only if profiling points there.

## 7. Multiple outputs and integrations

- [ ] Projection and confidence showing different sources (e.g. capture on confidence, video on projection) — measure headroom first.
- [ ] OBS output for lower thirds (current lyric as an HTTP endpoint or WebSocket feed).
- [ ] Network video streaming, multi-machine sync — deferred, no current need.

## 8. Remote control over the network

- [ ] Allow connections from the LAN with a configurable host and port (binds `127.0.0.1` today); add the Windows Firewall rule in the installer.
- [ ] Authentication (token or pairing).
- [ ] Throttle very frequent commands from remote clients (local commands already coalesce).

## 9. Automation

- [ ] Timeline mode: automated advance on a schedule or to the media clock, alongside manual cueing.
- [ ] Timeline scrubbing in the UI.

## 10. Web target (idea)

A browser-based operator or output, for example running the control panel from a tablet, or a browser output for a lobby screen. The Tauri mobile/library scaffolding was removed in 2026-10 to keep the build light; re-add it (`[lib]` crate type in `desktop/crates/app/Cargo.toml` plus a `lib.rs` entry point) if this goes ahead. The WebSocket remote and the browser-only UI mode are the natural starting points.

## 11. Release polish

- [ ] Friendly product name in the installer and Start menu; bump the version each build.
- [ ] Code signing (SmartScreen and Defender warnings).
- [ ] macOS: a separate validation pass before claiming support.
- [ ] Native Wayland on Linux (needs a Wayland subsurface for the output); XWayland works today.
- [ ] Clearer errors in the UI; logs reachable from the app.

## Not planned

HDR, genlock (hardware-synchronized scanout across monitors) and a general timeline editor are out of scope unless a real need appears.
