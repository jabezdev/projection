# Magi Projection

Software for a church worship team: a song library the team prepares during the week (web), and **PALTO**, the projection app that runs the service from its own local copy.

**Download:** the Windows installer is on the [Releases page](https://github.com/jabezdev/projection/releases/latest). It is unsigned, so Windows may warn (More info → Run anyway). PALTO then updates itself.

## What PALTO does

- Lyrics, images, video, audio, image and PDF slides, and live screen or window capture, cued one at a time on a **projection** screen and a stage **confidence** screen, each with its own style.
- **Black**, **Clear** and **Logo** keys; crossfade, cut or slide transitions; text shadow, outline and glow; `:icon:` codes ([Lucide](https://lucide.dev/icons)); built-in moving backgrounds.
- A song library and a service editor with sections, service-only song variations and Undo; quick search (Ctrl+K); OpenLyrics import.
- Keyboard, presenter clickers, MIDI, Stream Deck, a paired phone remote (`palto.local`) and OSC (Bitfocus Companion).
- Built for Sunday: fully offline, crash recovery to the last cue, keeps displays awake, reconnects HDMI audio, weekly backups, updates that never install while the screens are shown.

Best video format: MP4, H.264 8-bit 4:2:0, 1080p or smaller, AAC audio. Other formats play but cost more.

## Repository

| Folder | What | Built with |
|---|---|---|
| [desktop/](desktop/) | PALTO, the projection app | Rust, Tauri, wgpu, FFmpeg 9, React |
| [web/](web/) | Song library web app | React PWA, Convex, Clerk |

The two are independent projects with their own lockfiles and releases. How they work: [ARCHITECTURE.md](ARCHITECTURE.md). What's next: [ROADMAP.md](ROADMAP.md).

## Desktop (`desktop/`)

**UI only, in a browser** (no Rust needed): `cd desktop/crates/app/frontend && bun install && bun run dev`, then open http://localhost:5174. A built-in demo backend (`src/demo/`) stands in for the engine and loads a service with every item type; nothing is projected.

**Full app.** Needs Rust, Bun, FFmpeg 9 and the [Tauri system libraries](https://tauri.app/start/prerequisites/) (Arch: `sudo pacman -S ffmpeg webkit2gtk-4.1 base-devel`).

```bash
cd desktop/crates/app
bun install && bun install --cwd frontend
bun run dev
```

The projection and confidence windows start hidden; switch them on in the right column. Linux runs through XWayland (`GDK_BACKEND=x11`).

| What | Command (from `desktop/`) |
|---|---|
| Rust tests | `cargo test --workspace` |
| UI build, lint, tests | `cd crates/app/frontend && bun run build && bun run lint && bun run test` |
| Regenerate protocol fixtures after a wire change | `UPDATE_PROTOCOL_FIXTURES=1 cargo test -p protocol --test typescript` |
| Output parity check (needs ImageMagick) | `tools/parity/run.sh` |
| End-to-end, remote, soak, profile (needs a built app) | `bun tools/e2e/{e2e,remote,soak,profile}.ts` |
| Windows installer, cross-built from Linux | `tools/build-windows.sh` (bumps `0.MINOR.BUILD`) |
| Publish it as a GitHub release | `tools/release.sh` |

CI runs the Rust tests, the UI checks and the web checks on every push; a `v*` tag cross-builds and publishes the Windows installer.

**Data** lives in `%APPDATA%\com.magi.projection\` (Linux: `~/.local/share/com.magi.projection/`): `app.db` (library and services), `media/`, the settings JSON files, `logs/palto.log`, `backups/`. `PALTO_DATA_DIR=<folder>` uses another folder.

**Windows builds** come from `tools/build-windows.sh`, which fetches its toolchain without sudo (rustup into `~/.cargo`, `cargo-xwin`, the BtbN FFmpeg 9.0 LGPL build and NSIS into `~/.local/share/projection-win`) and needs `clang`, `lld`, `llvm` and `bsdtar`. The installer is `target/x86_64-pc-windows-msvc/release/bundle/nsis/PALTO_<version>_x64-setup.exe`.

**Releases.** Updates are signed with `~/.tauri/palto-updater.key` (public half in `tauri.conf.json`); keep a copy safe, since installed copies accept no other key. Locally: `RELEASE_NOTES="…" tools/build-windows.sh`, push the version bump, then `tools/release.sh`. From CI: push an annotated tag whose message is the notes; this needs the key as the `TAURI_SIGNING_PRIVATE_KEY` secret.

**Stream Deck on Linux** needs a udev rule: `SUBSYSTEM=="hidraw", ATTRS{idVendor}=="0fd9", TAG+="uaccess"`.

## Web (`web/`)

From `web/`, with Bun:

1. Create a Clerk app with a **Convex** JWT template; copy its Issuer URL.
2. `bun install && bun run dev:backend` (creates the Convex project), then set `CLERK_JWT_ISSUER_DOMAIN` to the Issuer URL in the Convex dashboard.
3. Copy `app/.env.example` to `app/.env.local` and fill in `VITE_CONVEX_URL` and `VITE_CLERK_PUBLISHABLE_KEY`.
4. `bun run dev:app`, then open http://localhost:5175. The first person to sign in becomes admin.

Other commands: `bun run typecheck`, `bun run test`, `bun run build` (output in `app/dist`), `bun run deploy:backend`.

## License

MIT. Bundled: [Lucide](https://lucide.dev) icons and font (ISC, `desktop/crates/render_engine/assets/lucide/LICENSE`).
