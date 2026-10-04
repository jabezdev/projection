# Roadmap

`[x]` done · `[/]` partly done · `[ ]` not started

## Where things stand

- [x] Projection output window, drawn on the GPU
- [x] Text and image backgrounds on the output
- [/] Video backgrounds: play and loop, but timing is approximate (no frame-accurate sync, no seeking)
- [x] Session playlist of mixed items with Next/Prev (steps within an item, then the next item)
- [x] Control panel, projection output and confidence (stage) windows
- [x] Projects saved in SQLite; create, list and load from the control panel
- [x] Settings saved between runs; clean shutdown
- [x] Remote control over WebSocket using the same commands/events as the UI (same machine only)
- [x] Engine covered by automated tests (`cargo test -p core_engine`)

### Known gaps

- Presentation items exist in the data model but don't display yet.
- Seek updates the clock but doesn't move the video.
- The browser-only UI mode fakes the engine; its Next/Prev doesn't move between items exactly like the real one.

## Next up

### Phase 1: Run a service manually
- [/] Controls specific to each item type (text steps done; image/video/presentation controls pending)
- [ ] Choose which monitor the output and confidence windows use, and fullscreen behaviour
- [ ] Output for OBS lower-thirds (current text as an HTTP endpoint or WebSocket feed)

### Phase 2: Editing projects
- [ ] Editor UI for items: add, reorder, edit text steps
- [ ] Decide how media files are handled (copied into a project folder vs. referenced by path)

### Phase 2b: Song library web app ↔ projection app
- [x] Convex backend: songs, sections, services, arrangements with per-service variations, roles
- [x] Web app (PWA): Library, Services (arrange + singers' lyrics view), Team (roles, pairing codes)
- [x] Pairing codes for projection computers (backend + web side)
- [ ] Deploy: Convex production deployment + host `web/app` (Vercel/Netlify/Cloudflare Pages)
- [ ] `protocol`: string IDs (Convex IDs), a `Song` item kind that keeps section labels, active position tracked by entry key
- [ ] Desktop: pairing screen, service picker, push the service into the engine, SQLite as offline cache
- [ ] Desktop edits use the same Convex functions (two-way)
- [ ] Desktop writes "now showing" to Convex; singers' view highlights it
- [ ] Generate TS types from `protocol` (ts-rs/specta) instead of hand-copying them
- [ ] Nice-to-have: drag-and-drop arranging, chords/keys for singers, media library in Convex storage

### Phase 3: Remote control over the network
- [ ] Allow connections from other machines on the LAN, with a configurable port
- [ ] Authentication (token or pairing)
- [ ] Throttle very frequent commands (e.g. scrubbing)

### Phase 4: Timeline / automated playback
- [ ] Audio playback with a precise master clock (`AudioEngine` is in place but not wired up)
- [ ] Frame-accurate video sync, seeking and smoother looping
- [ ] Manual vs. automated modes; timeline scrubbing in the UI

### Phase 5: Multiple outputs
- [ ] Let the projector and confidence screens show different things (e.g. overlays only on confidence)
- [ ] Web target (Tauri's scaffolding is kept in `desktop/crates/app/src/lib.rs` for this)

### Phase 6: Polish
- [ ] Transitions (crossfade etc.)
- [ ] Performance pass (frame pacing, texture uploads, video decoding)
- [ ] Hardening: clearer errors in the UI, logging
