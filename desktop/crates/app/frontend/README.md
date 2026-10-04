# Frontend (control panel)

React + TypeScript + Vite + Tailwind. This is the operator's control panel and the confidence (stage) view.

## Run

```bash
bun install
bun run dev      # http://localhost:5173 and /confidence
bun run build    # type-check + production build (used by the desktop app)
bun run lint
```

To run it inside the real desktop app instead, see the main [README](../../../README.md).

## Files that matter

- **`src/App.tsx`**: the whole UI: playlist, transport buttons, project list, confidence view.
- **`src/tauri.ts`**: the bridge to the engine. `invoke(command)` sends a command; `listen('engine:event')` receives updates. In a plain browser there is no engine, so this file fakes one (demo data lives in local storage).

## How the UI talks to the engine

- On startup it fetches the current state once (`get_engine_snapshot`), then listens for events.
- Events: `project_changed` and `program_changed` each contain the **complete** current state, so the UI just replaces what it has. `error` means a command failed.
- The UI never decides what's on air itself; it asks the engine and shows what the engine reports.

The command names and data shapes are defined in `crates/protocol/src/lib.rs`; the TypeScript types in `App.tsx` mirror them.
