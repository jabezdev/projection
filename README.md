# Magi Projection

Software for a church worship team: a song library the team prepares during the week, and a projection app that puts it on screen on Saturday and Sunday.

| Doc | What |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | How the two apps fit together; offline-first with cloud sync |
| [ENGINE.md](ENGINE.md) | How the projection app's outputs, media and engine work, and why |
| [ROADMAP.md](ROADMAP.md) | What's done and what's next |
| [WINDOWS.md](WINDOWS.md) | Windows release checklist for the church PC |
| [docs/records/](docs/records/) | Frozen design records (history) |

The repo holds two separate projects that share one git history. Each has its own dependencies, lockfile, tooling and deploys.

| Folder | What | Built with |
|---|---|---|
| [desktop/](desktop/) | Projection app the operator runs at the venue | Rust, Tauri, wgpu, FFmpeg, React |
| [web/](web/) | Song library web app: lyrics, service arrangements, roles | React PWA, Convex, Clerk |

---

## Projection app (`desktop/`)

The operator works in a web-style control panel. The projection and confidence screens are native GPU windows (no webview); one media session decodes each video once for both and owns the only audio output. Details in [ENGINE.md](ENGINE.md).

### Run the UI only, in a browser

Fast, no Rust needed; best for layout and styling work. Needs [Bun](https://bun.sh/).

```bash
cd desktop/crates/app/frontend
bun install
bun run dev
```

Open http://localhost:5174. There is no engine in this mode: [src/tauri.ts](desktop/crates/app/frontend/src/tauri.ts) fakes it and keeps demo projects in local storage. Buttons work, but nothing is projected. The web app uses port 5175, so both can run at once.

### Run the full desktop app

Needs:

- **Rust**: https://rustup.rs
- **Bun**: https://bun.sh
- **FFmpeg 8.x** and the **Tauri system libraries** (https://tauri.app/start/prerequisites/):
  - Arch: `sudo pacman -S ffmpeg webkit2gtk-4.1 base-devel`
  - Debian/Ubuntu: `sudo apt install libavcodec-dev libavformat-dev libavutil-dev libswresample-dev libswscale-dev pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`

```bash
cd desktop/crates/app
bun install
bun install --cwd frontend
bun run dev        # builds everything and opens the app
```

The first build takes several minutes. Three windows exist: **PAKIT** (the control panel), and the **projection** and **confidence** outputs, which start hidden; show them from Output Settings → Output displays. On Linux, `bun run dev` forces X11 (`GDK_BACKEND=x11`, XWayland under Wayland sessions) because native Wayland output is not supported.

### How the UI talks to the engine

- On startup the UI fetches the current state once (`get_engine_snapshot`), then listens for `engine:event`.
- `project_changed`, `program_changed` and `playback_changed` each carry the **complete** state of their domain, so the UI replaces what it has. `error` means a command failed.
- The UI never decides what is on air; it asks the engine and shows what the engine reports.
- Commands and data shapes are defined in [protocol/src/lib.rs](desktop/crates/protocol/src/lib.rs). [protocol.fixtures.ts](desktop/crates/app/frontend/src/protocol.fixtures.ts) is generated from Rust and checked against the UI's TypeScript types by the UI build; an error there means the two disagree.

### Everyday commands

From `desktop/` unless noted.

| What | Command |
|---|---|
| Check the Rust code compiles | `cargo check` |
| Run every Rust test | `cargo test --workspace` |
| Type-check, build, lint and test the UI | `cd crates/app/frontend && bun run build && bun run lint && bun run test` |
| Regenerate protocol fixtures after a wire change | `UPDATE_PROTOCOL_FIXTURES=1 cargo test -p protocol --test typescript` |
| Compare outputs with the frozen parity references | `tools/parity/run.sh` (needs ImageMagick) |
| Build an installable app | `cd crates/app && bun tauri build` |
| Build the Windows installer from Linux (bumps the version) | `tools/build-windows.sh` |
| Bump the version by hand (`0.MINOR.BUILD`) | `tools/bump-version.sh [minor]` |
| End-to-end check over the WebSocket (needs a built app) | `bun tools/e2e/e2e.ts` |
| 30-minute soak (memory, decoders, frames) | `bun tools/e2e/soak.ts --video <file>` |

CI (`.github/workflows/ci.yml`) runs the Rust tests, the UI build/lint/test and the web typecheck/tests on every push; a `v*` tag also cross-builds the Windows installer and attaches it to a GitHub release. The parity check is not in CI because its results depend on the installed fonts.

### Where your data lives

In the OS app-data folder, never in the repo:

- Linux: `~/.local/share/com.magi.projection/`
- Windows: `%APPDATA%\com.magi.projection\`

`app.db` holds the library and services; `app-config.json` the WebSocket host/port; `output-settings.json` and `output-displays.json` output styling and display assignments; `session-state.json` the last cue, for crash recovery. `logs/` holds `pakit.log` and up to four older logs (Settings → System → Open logs folder). Existing MVP installs keep using the `com.tauri.dev` folder when it contains the library; logs always go to `com.magi.projection`. Imported media references its original file; keep those files in place (copying media into the library is planned, roadmap phase 3). `PAKIT_DATA_DIR=<folder>` runs the app on another data folder, as the test scripts do.

**Backups.** Settings → System → Export backup… writes a zip with a consistent copy of `app.db`, the settings files and `media.json` (every media file the library uses, which services use it and whether it still exists; the media files themselves are not included). Once a week the app also writes `backups/auto-<date>.zip` and keeps the newest five. To restore, close PAKIT and unzip `app.db` and the settings files into the data folder.

### Folder layout

```
desktop/
  crates/
    app/            The desktop app: windows, UI commands, startup, storage worker
      frontend/     React control panel
    core_engine/    The engine: decides what's on screen and which media plays
    render_engine/  Native output drawing (one GPU device, a thread per output)
    media_engine/   Decoding, audio output and playback clocks (FFmpeg)
    protocol/       The shared list of commands, events and settings
    persistence/    Saving/loading songs and services (SQLite)
    network_server/ Remote control over WebSocket
  tools/
    parity/         Output regression check against frozen references
    e2e/            End-to-end and soak scripts over the WebSocket (Bun)
    bump-version.sh Version bump (0.MINOR.BUILD)
    build-windows.sh  Cross-builds the Windows installer
  assets/           Sample backgrounds
```

"Crate" is Rust's word for a package; each folder builds separately.

### Using it

**Operator layout.** Three resizable columns (30/45/35). Settings are at the top left. Selecting a song part on the left jumps to its cue in the center; extra scroll space keeps the last cue reachable. The right column holds output controls and two previews: still images of exactly what each output shows, refreshed when a cue, style or size changes (they do not play video).

**Media cards.** Standalone video and audio cards in the center have play/pause, stop, seek, volume, mute, loop, clip start/end and speed, saved with the service. Video cards also have a Confidence checkbox that shows the video on the confidence screen too (muted there; audio plays once). Song and Logo video backgrounds loop muted without transport controls.

**Displays.** Output display assignments and fullscreen/windowed choices are saved and restored by display identity. A missing display stays saved and shows as disconnected. If the connected displays change while Settings is open, reopen Settings before placing an output.

**Audio device.** Choose the venue output in the OS sound settings. If it disappears, the media card shows a reconnecting message and playback resumes on the new default device.

**Not there yet.** There are no keyboard shortcuts for cueing (the mouse runs the service), no undo, and output styling is shared by both screens except the confidence text size. Configurable shortcuts and outside devices (clickers, MIDI, Stream Deck, OSC), an edit history with undo, operator-designed service templates and per-screen settings are planned; see [ROADMAP.md](ROADMAP.md).

### MVP rehearsal

Build and launch the production executable:

```bash
cd desktop/crates/app
bun install
bun install --cwd frontend
bun run tauri build --no-bundle
GDK_BACKEND=x11 ../../target/release/app     # Linux
```

1. **Songs and services.** Open Library, create a song, edit its parts/arrangement and save. Create a service, add the song, reorder items, rename it and save. Restart and reopen it. Library songs and service arrangements are separate copies; edit the service copy with its pencil button.
2. **Images and Logo.** Import images from Library → Images. Give a scheduled song an image or video background. Add an image directly to the service and cue it. In Output Settings choose a logo image or video, press Logo (logo videos loop muted), then Live to return to lyrics.
3. **Presentations.** Create an image presentation, choose and reorder several images, save, and cue each thumbnail. Edit and reopen it to check the order persists.
4. **Displays and lyrics.** In Output Settings → Output displays, assign the projector and confidence displays and press Show fullscreen for each (Show windowed to inspect). Advance lyrics: old and new text fade together using the configured fade while the border and box fill stay; confidence scrolls the active slide into view. Try rapid navigation.
5. **Video.** Import a video with sound, add it and cue it. Check play/pause, stop, seek, loop, volume, mute and speed. Backgrounds keep the outgoing picture until the new one is ready, then crossfade. Try the actual files you intend to use.
6. **Audio.** Import an audio file, add it and cue it. The audio card has the same transport; the projection keeps the song background, and audio keeps playing under the Logo.
7. **Switching.** Move between lyrics, images, video, Logo, Clear and Black. Both outputs recover, and a video stops sounding when you leave its cue.

Automated checks do not certify smoothness, monitor placement or venue audio; only a real multi-display rehearsal does.

### Packaging

Bundles appear under `desktop/target/release/bundle/`. Platform settings: [tauri.linux.conf.json](desktop/crates/app/tauri.linux.conf.json), [tauri.windows.conf.json](desktop/crates/app/tauri.windows.conf.json).

**Linux** (`.deb`, AppImage): `cd desktop/crates/app && bun tauri build`. The `.deb` depends on the FFmpeg 8 runtime libraries (`libavcodec62`, `libavformat62`, `libavutil60`, `libswresample6`, `libswscale9`); the AppImage carries them. On Arch, use `NO_STRIP=true bun tauri build`: linuxdeploy's `strip` cannot read current system libraries.

**Windows** (NSIS installer; MSI only when built on Windows). The app links FFmpeg 8 dynamically and needs only `avcodec`, `avformat`, `avutil`, `swresample` and `swscale`; the installer places those DLLs next to `app.exe` and downloads WebView2 (for the control panel) if missing.

*From Linux* (the usual way): `desktop/tools/build-windows.sh`. The installer appears at `desktop/target/x86_64-pc-windows-msvc/release/bundle/nsis/PAKIT_<version>_x64-setup.exe` (installed as `PAKIT.exe`; the script bumps the build number first, `--no-bump` skips that). The script needs `clang`, `lld`, `llvm` and `bsdtar`; it fetches the rest without sudo: rustup into `~/.cargo` (the system Rust stays default), `cargo-xwin` (downloads the MSVC SDK on first use), the BtbN FFmpeg 8.1 LGPL shared build and Debian's NSIS, into `~/.local/share/projection-win`. The installer is unsigned: SmartScreen asks for confirmation (More info → Run anyway).

*On Windows* (not yet tried):

1. Install Rust (MSVC toolchain), Bun, and LLVM/Clang (the FFmpeg bindings use `bindgen`).
2. Unpack an FFmpeg 8.x **shared, LGPL** win64 build, e.g. to `C:\ffmpeg` (with `include\`, `lib\`, `bin\`), and set `$env:FFMPEG_DIR = "C:\ffmpeg"`.
3. Copy the five runtime DLLs into `desktop\crates\app\windows\ffmpeg\` (git-ignored):
   ```powershell
   New-Item -ItemType Directory -Force crates\app\windows\ffmpeg
   Copy-Item C:\ffmpeg\bin\avcodec-*.dll, C:\ffmpeg\bin\avformat-*.dll, C:\ffmpeg\bin\avutil-*.dll, C:\ffmpeg\bin\swresample-*.dll, C:\ffmpeg\bin\swscale-*.dll crates\app\windows\ffmpeg\
   ```
4. `cd crates\app; bun install; bun install --cwd frontend; bun tauri build`.

Install on a machine without FFmpeg or developer tools, then run the rehearsal. Remaining Windows checks are in [WINDOWS.md](WINDOWS.md). Updates are installed by hand for now; an auto-updater that downloads in the background, asks before installing and can roll back is planned (roadmap phase 11).

---

## Song library (`web/`)

A phone-friendly site (installable as an app) where the week's prep happens: the lyric team adds songs, the song leader arranges the service, singers read the arrangement. Data lives in [Convex](https://convex.dev) and sign-in uses [Clerk](https://clerk.com). Data model: [ARCHITECTURE.md](ARCHITECTURE.md#cloud-data-model). Today the web app and the projection app are separate: services are entered on the desktop by hand. Next, the library is reworked so library songs stay untouched references (fixes reach upcoming services only), then the projection app syncs offline-first with Convex, including media up to 100 MB ([roadmap](ROADMAP.md#3-library-revamp-and-data-model), phases 3–4).

All commands run from `web/`. Needs Bun.

### First-time setup

1. Create a Clerk application. In Clerk's dashboard: **JWT templates → New → Convex** (keep the name `convex`) and copy its **Issuer** URL.
2. Start the backend; the first run asks you to log in to Convex and create a project:
   ```bash
   bun install
   bun run dev:backend
   ```
   In the Convex dashboard → Settings → Environment variables, set `CLERK_JWT_ISSUER_DOMAIN` to the Issuer URL.
3. Copy `app/.env.example` to `app/.env.local` and fill in `VITE_CONVEX_URL` (printed by step 2, also in `backend/.env.local`) and `VITE_CLERK_PUBLISHABLE_KEY` (Clerk → API keys).
4. In a second terminal: `bun run dev:app`, then open http://localhost:5175.

**The first person to sign in becomes admin.** Everyone after starts as a *singer* (read-only) until an admin gives them a role on the **Team** tab.

### Commands

| What | Command |
|---|---|
| Run the backend (watches and pushes changes) | `bun run dev:backend` |
| Run the web app | `bun run dev:app` |
| Typecheck everything | `bun run typecheck` |
| Run all tests | `bun run test` |
| Build the web app (output in `app/dist`) | `bun run build` |
| Deploy the backend to production | `bun run deploy:backend` |

### Deploying

- **Backend:** `bun run deploy:backend` pushes the Convex functions and schema to production.
- **Web app:** point your host (Vercel, Netlify, Cloudflare Pages) at this repo with root directory `web`, build command `bun run build`, output directory `app/dist`. Set `VITE_CONVEX_URL` (production) and `VITE_CLERK_PUBLISHABLE_KEY` in the host's settings.

### Folder layout

```
web/
  app/        The web app (React PWA)
  backend/    Convex: schema (convex/schema.ts) and server functions
  shared/     Lyric parsing and slide splitting, for the web app and later the desktop
```

## License

MIT
