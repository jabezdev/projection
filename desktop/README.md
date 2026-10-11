# Projection app (desktop)

A desktop app for projecting lyrics, images and video during a live event. The operator works in a web-style control panel. The projection and confidence outputs are native GPU windows (see [Output engine](#output-engine)).

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

Open http://localhost:5174 (control panel) or http://localhost:5174/confidence (stage monitor view).

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

### Output engine

Projection and confidence are plain OS windows drawn by [render_engine](crates/render_engine/) on the GPU. One [media_engine](crates/media_engine/) session decodes each video once for both outputs and owns the only audio output (the system default device). Operator previews are still images rendered by the same engine.

The previous webview output engine was removed on 2026-10-11. Services, songs and settings saved by either engine load unchanged.

## Where your data lives

Projects and settings are stored in the OS app-data folder, never in the repo:

- Linux: `~/.local/share/com.magi.projection/`
- Windows: `%APPDATA%\com.magi.projection\`

`app.db` holds the library and services; `app-config.json` holds the WebSocket port; `output-settings.json` and `output-displays.json` hold output styling and display assignments. Existing MVP installs continue using the `com.tauri.dev` folder when it contains the library.

## Everyday commands

| What | Command |
|---|---|
| Check the Rust code compiles | `cargo check` |
| Run every Rust test | `cargo test --workspace` |
| Type-check, build and lint the UI | `cd crates/app/frontend && bun run build && bun run lint && bun run test` |
| Compare native outputs with the frozen webview references | `tools/parity/run.sh` (needs ImageMagick) |
| Regenerate protocol fixtures after a wire change | `UPDATE_PROTOCOL_FIXTURES=1 cargo test -p protocol --test typescript` |
| Build an installable app | `cd crates/app && bun tauri build` |

`crates/app/frontend/src/protocol.fixtures.ts` is generated from Rust: each serialized sample is checked against the UI's TypeScript types by the UI build. If the build reports an error there, the Rust protocol and the TypeScript types disagree.

## Folder layout

```
crates/
  app/            The desktop app: windows, UI commands, startup, storage worker
    frontend/     React control panel
  core_engine/    The engine: decides what's on screen and which media plays
  render_engine/  Native output drawing (one GPU device, a thread per output)
  media_engine/   Native decoding, audio output and playback clocks (FFmpeg)
  protocol/       The shared list of commands and events
  persistence/    Saving/loading projects (SQLite)
  network_server/ Remote control over WebSocket
tools/parity/     Native output regression check against frozen webview references
assets/           Sample backgrounds
```

"Crate" is Rust's word for a package; each folder above builds separately and they depend on each other.

## MVP rehearsal

From the repository root, build and launch the production executable:

```bash
cd desktop/crates/app
bun install
bun install --cwd frontend
bun run tauri build --no-bundle
GDK_BACKEND=x11 ../../target/release/app
```

For an installer, run `bun run tauri build` instead. Bundles appear under `desktop/target/release/bundle/`.

1. Open Library, create a song, edit its parts/arrangement, and save. Create a service, add the song, reorder its items, edit its name, and save. Restart and reopen it to check persistence. Library songs and service song arrangements are separate copies; edit the service copy with its pencil button.
2. Import images from Library → Images. Edit a scheduled song and choose an image or video background. Add an image directly to the service and cue it. In Output Settings choose a logo image or video, then press Logo; logo videos loop muted with a static operator thumbnail; press Live to return to lyrics.
3. Create an Image presentation, choose multiple images, reorder them, save, and cue each thumbnail. Edit and reopen the presentation to check its order persists.
4. In Output Settings → Output displays, select the projector and confidence displays and press Show fullscreen for each. Use Show windowed to reposition or inspect them. Advance lyrics: the old and new text fade simultaneously using the configured fade duration while the border and box fill remain visible; confidence smoothly scrolls the active slide into view. Check both during rapid navigation.
5. Import a video with audible sound, add it to the service, and cue it. Verify Play/Pause, Stop, seek, loop, volume, mute, and speed. Standalone videos autoplay with audio on the projection window. The operator displays a static thumbnail and sends transport commands to the projector; it does not decode video. Background transitions retain the outgoing frame until the incoming media has loaded, then crossfade. Song video backgrounds autoplay muted, fill the screen, and loop without transport controls. Under the webview engine, video streaming uses an automatically started loopback HTTP server with byte-range support, and codec availability depends on the system webview. Thumbnails are decoded in-process and cached. Select the venue audio device through the OS sound settings; under the native engine, unplugging it shows a reconnecting message in the media card and playback resumes on the new default device. Try the actual media files you intend to use.
7. Import an audio file, add it to the service and cue it. The center column shows an audio card with the same transport, loop, clip and speed controls as video; the projection keeps the song background. Audio keeps playing under the Logo.
6. Switch between lyrics, images, video, Logo, Clear, and Black. Confirm both outputs recover and the video stops sounding when you leave its cue.

A real multi-display rehearsal is required to verify smoothness, monitor placement, and venue audio. Automated checks do not certify those hardware behaviors. Imported media references its original file; keep those files in place.

## Operator layout and video cards

The initial columns use relative 30/45/35 widths and can be resized. Settings are at the top left. Selecting a song part in the left column jumps to its cue in the center; selecting center cues leaves the scroll position unchanged, and extra scroll space keeps the last cue accessible near the center. The two output previews follow their output windows' aspect ratios. Under the native engine they are still images of what each output shows, refreshed when a cue, style or size changes; under the webview engine they are approximations with video posters. Neither plays video.

Standalone video cards in the center contain play/pause, stop and seek. The video cards save start/end times (seconds), looping, volume, speed and a Confidence checkbox with the service. Confidence video is muted to keep audio on the main projector. Standalone audio cards have the same transport and saved options, without the Confidence checkbox. Song and logo video backgrounds continue to loop without transport controls. Volume and mute live in the center video card beside playback; the right column is reserved for output controls and previews. Confidence preloads the next enabled video, crossfades only once it is ready, follows transport commands immediately, and corrects small timing drift through playback speed rather than repeated seeks.

Output display assignments and fullscreen/windowed choices are saved separately and restored on restart using display identity. A missing display remains saved and appears as disconnected in Settings. If the connected displays change while Settings is open, placing an output asks you to reopen Settings rather than using a stale list.

## Packaging

Bundles appear under `desktop/target/release/bundle/`. Platform settings live in [tauri.linux.conf.json](crates/app/tauri.linux.conf.json) and [tauri.windows.conf.json](crates/app/tauri.windows.conf.json).

**Linux** (`.deb`, AppImage): `cd crates/app && bun tauri build`. The `.deb` depends on the FFmpeg 8 runtime libraries (`libavcodec62`, `libavformat62`, `libavutil60`, `libswresample6`, `libswscale9`); the AppImage carries them. On Arch-based systems, build the AppImage with `NO_STRIP=true bun tauri build`: linuxdeploy's `strip` cannot read current system libraries.

**Windows** (NSIS installer and MSI). The app links FFmpeg 8 dynamically; only `avcodec`, `avformat`, `avutil`, `swresample` and `swscale` are needed. The installer places those five DLLs next to `app.exe` and downloads WebView2 if it is missing.

*From Linux* (NSIS installer only; the MSI needs WiX, which runs only on Windows): `tools/build-windows.sh`. The installer appears at `target/x86_64-pc-windows-msvc/release/bundle/nsis/projection-rust_<version>_x64-setup.exe`. The script needs `clang`, `lld`, `llvm` and `bsdtar` from the system; it fetches everything else without sudo: rustup into `~/.cargo` (the system Rust stays the default), `cargo-xwin` (which downloads the MSVC SDK on first use), the BtbN FFmpeg 8.1 LGPL shared build and Debian's NSIS, both into `~/.local/share/projection-win`. The installer is unsigned, so Windows SmartScreen asks for confirmation (More info → Run anyway).

*On Windows:*

1. Install Rust (MSVC toolchain), Bun, and LLVM/Clang (the FFmpeg bindings use `bindgen`).
2. Download an FFmpeg 8.x **shared, LGPL** build for win64 and unpack it, for example to `C:\ffmpeg`. It must contain `include\`, `lib\` and `bin\`.
3. In the shell that builds: `$env:FFMPEG_DIR = "C:\ffmpeg"`.
4. Copy the five runtime DLLs into `crates\app\windows\ffmpeg\` (git-ignored); the installer places them next to `app.exe`:
   ```powershell
   New-Item -ItemType Directory -Force crates\app\windows\ffmpeg
   Copy-Item C:\ffmpeg\bin\avcodec-*.dll, C:\ffmpeg\bin\avformat-*.dll, C:\ffmpeg\bin\avutil-*.dll, C:\ffmpeg\bin\swresample-*.dll, C:\ffmpeg\bin\swscale-*.dll crates\app\windows\ffmpeg\
   ```
5. `cd crates\app; bun install; bun install --cwd frontend; bun tauri build`.

Install the result on a machine without FFmpeg or developer tools, then run the rehearsal above. The Linux cross-build was first produced on 2026-10-11 and has not yet been run on Windows; the on-Windows steps have not been run either. Record what had to change.
