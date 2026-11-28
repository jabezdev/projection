# Magi Projection

**PALTO** (`desktop/`, Rust + Tauri + wgpu + FFmpeg 9 + React) runs the service offline. The team library (`web/`, React + Convex + Convex Auth) is where the team prepares it, and PALTO syncs with it. The OBS plugin (`obs/`, Rust) shows PALTO's lyrics and song credits on the livestream. Windows installer: [Releases](https://github.com/jabezdev/projection/releases/latest).

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

In `web/`: `bun install && bun run dev:backend` (choose "Start without an account" to run Convex locally), copy the printed URL into `app/.env.local` as `VITE_CONVEX_URL`, then `bun run dev:app`. The first account becomes the admin.

- Tests: `bun run test`. Sync against a running backend: `PALTO_SYNC_URL=… PALTO_SYNC_CODE=… cargo test -p app e2e -- --ignored`
- Production: create a Convex project and a Cloudflare Pages project `palto-library`, add `CONVEX_DEPLOY_KEY`, `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` as repository secrets, then run the "Deploy web library" workflow (or `bun run deploy` with those set).
- Pairing: on the web, Team → Projection computers gives the team address and a code; in PALTO, Settings → Cloud sync.

## Site

`site/` is the public page and setup guide: static HTML, no build. The download buttons read the latest release from the GitHub API. Deploy: a Cloudflare Pages project `palto-site` and the "Deploy PALTO site" workflow (also on pushes to `site/` once the repository variable `SITE_ENABLED` is `true`).

MIT. Lucide icons: ISC. Bundled Bibles (KJV, Ang Dating Biblia 1905) are public domain, from [open-bibles](https://github.com/seven1m/open-bibles); `desktop/crates/bible/data/*.tsv.gz` is made from their OSIS files with the crate's own importer.
