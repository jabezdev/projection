# Magi Projection

**PALTO** (`desktop/`, Rust + Tauri + wgpu + FFmpeg 9 + React) runs the service offline. The song library (`web/`, React + Convex + Clerk) is where the team prepares it. The OBS plugin (`obs/`, Rust) shows PALTO's lyrics and song credits on the livestream. Windows installer: [Releases](https://github.com/jabezdev/projection/releases/latest).

## Desktop

Needs Rust, Bun, FFmpeg 9 and the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

```bash
cd desktop/crates/app && bun install && bun install --cwd frontend && bun run dev
```

- UI only, with a demo backend: `cd desktop/crates/app/frontend && bun run dev`
- Tests: `cargo test --workspace`, `bun run lint && bun run test` in `frontend/`
- Protocol fixtures: `UPDATE_PROTOCOL_FIXTURES=1 cargo test -p protocol --test typescript`
- Windows installer: `tools/build-windows.sh`, then `tools/release.sh`. Updates are signed with `~/.tauri/palto-updater.key`.
- Data: `%APPDATA%\com.magi.projection\`, or `PALTO_DATA_DIR`.

## Web

In `web/`: `bun install && bun run dev:backend`, set `CLERK_JWT_ISSUER_DOMAIN` in Convex, fill `app/.env.local` from `app/.env.example`, then `bun run dev:app`.

MIT. Lucide icons: ISC.
