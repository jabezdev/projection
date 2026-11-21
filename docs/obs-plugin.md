# OBS lyrics plugin (design, not built)

Plan for sending PALTO's on-air lyrics to OBS on the livestream computer. Agreed 2026-11-21; the work waits for the phase 3 data model so song credits can be included from the first release. The roadmap entry is [ROADMAP.md phase 12](../ROADMAP.md#12-obs-lyrics-plugin). When this is built, move the settled parts into ARCHITECTURE.md (piece table, "Where to change things") and keep this file as a record.

## Decisions

| Question | Decision | Why |
|---|---|---|
| Native plugin or an OBS Browser Source page | **Native OBS plugin** | A browser source runs Chromium (CEF) on the stream PC (roughly 100 MB or more); the system should stay light. A PALTO-hosted overlay page can still be added later. |
| Language | **Rust**, a `cdylib` with a thin hand-written FFI to libobs | Same toolchain as the desktop. It can reuse the feed types and `tokio-tungstenite`, and it cross-builds for Windows with `cargo-xwin` on Linux, like the desktop installer. |
| Pairing | **None.** The feed is read-only and open on the LAN | The stream PC should just work. Lyrics are public on the projection screen anyway. Commands still need pairing, so opening the feed never opens control. |
| Finding PALTO | **Scan the network** from the plugin: mDNS browse first, then a subnet probe | No typing IP addresses at church. |
| Song credits | **Included**: title plus author, copyright and CCLI number for the stream credit line | Streaming licences expect song credits on screen. This needs new song fields, so it waits for the phase 3 data model. |

## Desktop side (PALTO)

### Feed endpoint
- **`GET /feed/v1` as a WebSocket** on the remote server's port. It needs no token and sends no commands. Anything a feed client sends is ignored, or closes the connection.
- **Every message is the complete state**, sent on connect and on each change. It is built from `ProgramChanged` (and `ProjectChanged` for item and song details), never from playback position reports:
  ```json
  {
    "v": 1, "seq": 42,
    "display": "live",
    "text": "Amazing grace, how sweet the sound",
    "lines": ["Amazing grace,", "how sweet the sound"],
    "next_text": "That saved a wretch like me",
    "item": { "kind": "song", "title": "Amazing Grace" },
    "section": "Verse 1",
    "song": { "title": "Amazing Grace", "authors": ["John Newton"], "copyright": "Public Domain", "ccli": "22025" }
  }
  ```
  - `display` other than `live` (`clear`, `black`, `logo`) means the plugin blanks.
  - `seq` only grows, so stale messages can be dropped.
  - `song` is absent for non-song items, and each of its fields is optional.
- **The contract is `protocol/src/feed.rs`,** separate from `EngineEvent` (which may change whenever PALTO changes). It has fixtures in `desktop/fixtures/feed/` that both PALTO and the plugin test against.
- **Additive only once released**, the same rule as the Convex API in ARCHITECTURE.md: new optional fields only. A breaking change becomes `/feed/v2`, and v1 keeps being served.
- **`/api/check` adds `feed_versions: [1]`,** so the plugin can tell "PALTO too old" apart from "unreachable".

### Reachability
- **Open:** does the feed follow the existing **LAN access** switch, or get its own switch, e.g. "Share lyrics with OBS on the network"? The recommendation is its own switch, on by default once turned on the first time: the server would listen on all interfaces when either switch is on, and control would still need pairing. The Windows Firewall rule from phase 10 already covers the port.
- **The operator's Remote settings list connected feed clients** by address, e.g. "OBS on 192.168.1.30".
- **Rate:** at most one message per program change, which is a few per minute in practice. No throttling is needed beyond the existing per-connection limits.

### Discovery
- **Also advertise a dedicated mDNS service, `_palto._tcp.local.`,** next to today's `_http._tcp` one. Its TXT record holds `feed=1`, `name=<computer name>` and `version=<app version>`, so the plugin can list several PALTO computers by name.
- **For networks that block mDNS,** the plugin probes `/api/check` on the default port across its own /24 subnets (a short timeout, a few at a time). The response must identify PALTO, e.g. `"app": "palto"`.

### Song credits (waits for phase 3)
- **New song fields:** `authors` (list), `copyright`, `ccli` (CCLI song number), all optional.
- **Edited** in the song editor and the web library; OpenLyrics import fills them from `<authors>`, `<copyright>` and `<ccliNo>`.
- **Synced** with Convex like the rest of the song.

## OBS side (the plugin)

### Project
- **New top-level folder `obs/`,** an independent project like `desktop/` and `web/`: its own Cargo workspace, lockfile, CI job and releases.
- **Its only link to `desktop/`** is a path dependency on the feed types, or a vendored copy checked against the shared fixtures. Prefer whichever keeps the plugin build free of desktop dependencies.
- **Layout:**
  - `obs/src/ffi.rs` — the libobs declarations used, about 15 functions.
  - `obs/src/feed.rs` — connection, reconnect and state.
  - `obs/src/discover.rs` — mDNS and the subnet probe.
  - `obs/src/source.rs` — the OBS sources.
  - `obs/data/locale/en-US.ini` — the plugin's text strings.
- **Linking:** link against `obs.dll`/`libobs.so` through an import library generated from the OBS release, or loaded at run time. Pick whichever cross-builds cleanly with `cargo-xwin`.

### Sources
- **PALTO Lyrics:** the current lines.
- **PALTO Song Info:** the song title and credits line.
- **PALTO Section:** e.g. "Verse 1".
- All sources share **one connection per OBS process** for each PALTO computer.
- **Text is drawn by OBS's own text source,** held as a private child: `text_gdiplus` on Windows, `text_ft2_source` on Linux. The plugin only sets its text, so font, outline, background and so on are OBS's own properties, and there is no text renderer to maintain.
- **Fade in and out** is an opacity pass in the source's render callback, with a duration property.

### Properties
- **PALTO computer:** a list filled by scanning (name and address), a "Scan again" button, and a manual address field as a fallback.
- **A status line:** Connected / Searching / PALTO not found / PALTO too old, update it / Plugin too old, update it.
- **Display options:**
  - Blank on Clear, Black and Logo (on by default).
  - Fade duration.
  - Maximum lines.
  - Text case (as is / upper).
  - For Song Info, a credits template, e.g. `{title} · {authors} · CCLI #{ccli}`.

### Behaviour
- **Disconnects:** keep the last text for a few seconds, then blank, and reconnect with backoff. Nothing must stay on stream that is no longer on the projection screen for long.
- **Address changes:** remember the chosen computer by mDNS name. If its address changes, find it again by scanning.
- **Threading:** networking runs on the plugin's own thread or runtime, and the OBS render thread never waits on it. Text updates are handed over through a lock-free slot or a short mutex.

### Targets
- **OBS 30 or newer** (Qt6 era, the current plugin folder layout).
- **Windows x64** is the target that has to work, judged on the church's stream PC.
- **Linux x64** is for development: OBS 32 runs on the dev machine.

## Testing
- **Feed fixtures** are shared by PALTO and the plugin, so drift fails CI on both sides.
- **Plugin unit tests** cover the feed's state handling, blanking and reconnect logic without OBS.
- **`desktop/tools/e2e/feed.ts`:** cue, Clear, Black, Logo and item changes against a scratch PALTO, checking each feed message.
- **Manual checks:**
  - Linux OBS locally.
  - On the church setup: a separate stream PC, the firewall, mDNS on the church network, the subnet probe when mDNS is blocked, and a service-length soak.
  - Add these to the church-PC visit list in [ROADMAP.md](../ROADMAP.md#next-church-pc-visit) when built.

## Release
- **Order:**
  1. The PALTO side (feed, discovery, credits) ships first, in a normal desktop release.
  2. Then the plugin.
- **Versioning:** independent of PALTO, using the same `0.MINOR.BUILD` scheme. Tags are `obs-v0.1.0`, which do not match the desktop's `v*` release job. The README keeps a compatibility table: plugin version → feed version → minimum PALTO version.
- **Artifacts on a GitHub Release,** built in CI by cross-compiling on Linux:
  - An NSIS installer that installs into `%ProgramData%\obs-studio\plugins\palto-obs\` (`bin\64bit\palto-obs.dll` plus `data\`). The desktop build already has NSIS.
  - A zip for manual install.
  - A Linux tarball (unofficial).
- **No auto-update.** OBS has no plugin updater, and the stream PC may be offline. The status line tells the operator when the plugin or PALTO needs updating.
- **No code signing,** the same as PALTO (SmartScreen warns once).
- **Docs:** `obs/README.md` (install, choosing the PALTO computer, the sources), a line in the root README, and release notes from the annotated tag message, as for the desktop.

## Order of work
1. **Phase 3 data model** includes the song credit fields.
2. **`protocol/src/feed.rs`** and fixtures; `/feed/v1`; `/api/check` changes; the `_palto._tcp` advertisement; the feed reachability setting.
3. **`obs/` skeleton:** the plugin loads in OBS on Linux, registers a source, and shows a fixed text through the child text source.
4. **The feed client,** discovery and status; then fades and the Song Info template.
5. **Windows cross-build** and the installer; CI job; first `obs-v` release after a church check.
