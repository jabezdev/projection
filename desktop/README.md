# Projection app (desktop)

A desktop app for projecting lyrics, images and video during a live event. The operator works in a web-style control panel; the projector output is drawn natively on the GPU so it stays smooth.

Built with **Tauri** (desktop shell), **React + TypeScript** (control panel UI) and **Rust** (everything behind the UI). See [../ARCHITECTURE.md](../ARCHITECTURE.md) for how the pieces fit together and [../task.md](../task.md) for what's done and what's next.

All commands below run from this `desktop/` folder.

## Two ways to run it

### 1. UI only, in the browser (fast, no Rust needed)

Best for layout and styling work. Needs [Bun](https://bun.sh/).

```bash
cd crates/app/frontend
bun install
bun run dev
```

Open http://localhost:5173 (control panel) or http://localhost:5173/confidence (stage monitor view).

There is no real engine in this mode: [src/tauri.ts](crates/app/frontend/src/tauri.ts) fakes it and keeps demo projects in the browser's local storage. Buttons work, but nothing is projected.

### 2. The full desktop app

Needs:

- **Rust**: https://rustup.rs
- **FFmpeg 8.x** (video decoding) and the **Tauri system libraries**: see https://tauri.app/start/prerequisites/
  - Arch: `sudo pacman -S ffmpeg webkit2gtk-4.1 base-devel`
  - Debian/Ubuntu: `sudo apt install libavcodec-dev libavformat-dev libavutil-dev libswscale-dev pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`
  - macOS: `brew install ffmpeg`

```bash
cd crates/app/frontend && bun install   # UI dependencies
cd .. && bun install                    # Tauri command-line tool
bun run dev                             # builds everything and opens the app
```

The first build takes several minutes; later ones are much faster. Three windows open:

- **Projection Editor**: the control panel
- **Projection Output**: fullscreen output for the projector
- **Confidence**: what's on now / what's next, for the stage

`bun run dev` forces X11 (`GDK_BACKEND=x11`) because the output window is unstable under native Wayland.

## Where your data lives

Projects and settings are stored in the OS app-data folder, never in the repo:

- Linux: `~/.local/share/com.tauri.dev/` (`app.db` = projects, `app-config.json` = settings)

## Everyday commands

| What | Command |
|---|---|
| Check the Rust code compiles | `cargo check` |
| Run the engine's tests | `cargo test -p core_engine` |
| Lint the UI | `cd crates/app/frontend && bun run lint` |
| Build an installable app | `cd crates/app && bunx tauri build` |

## Folder layout

```
crates/
  app/            The desktop app: windows, UI commands, startup
    frontend/     React control panel
  core_engine/    The engine (decides what's on screen) + GPU drawing
  protocol/       The shared list of commands and events
  persistence/    Saving/loading projects (SQLite)
  network_server/ Remote control over WebSocket
assets/           Sample backgrounds
```

"Crate" is Rust's word for a package; each folder above builds separately and they depend on each other.
