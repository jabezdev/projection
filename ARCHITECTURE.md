# Architecture

PALTO: a React control panel in Tauri; the projection and confidence screens are native wgpu windows with no webview. `core_engine` decides what is on air. One `media_engine` session decodes each video once for both screens and owns the only audio output. `render_engine` draws, `protocol` holds the wire types and the shared data model (`protocol::model`, mirrored in Convex), `persistence` is SQLite and resolves a service into what the engine plays, `network_server` is the remote and OSC.

## Decisions

1. Native outputs only. If they fail, use a separate output process, never a webview.
2. One decoder and one audio output per session, shared by both screens.
3. The engine owns playback state. React is for editing and controls only.
4. Windows 10/11 on a Ryzen 3 2200G with a GTX 1050 Ti is the bar. Linux runs under XWayland.
5. Keep it light: bounded queues, redraw only on change, no per-frame JSON.
6. Offline-first: sync feeds the local library and never blocks a service.
7. Once released, Convex functions and the OBS feed only gain optional fields.
