# Architecture

PALTO: a React control panel in Tauri; the projection and confidence screens are native wgpu windows with no webview. `core_engine` decides what is on air. One `media_engine` session decodes each video once for both screens and owns the only audio output. `render_engine` draws, `protocol` holds the wire types and the shared data model (`protocol::model`, mirrored in Convex), `persistence` is SQLite and resolves a service into what the engine plays, `bible` holds the Bibles and reads references, `network_server` is the remote and OSC.

## Decisions

1. Native outputs only. If they fail, use a separate output process, never a webview.
2. One decoder and one audio output per session, shared by both screens.
3. The engine owns playback state. React is for editing and controls only.
4. Windows 10/11 on a Ryzen 3 2200G with a GTX 1050 Ti is the bar. Linux runs under XWayland.
5. Keep it light: bounded queues, redraw only on change, no per-frame JSON.
6. Offline-first: sync feeds the local library and never blocks a service.
7. Once released, Convex functions and the OBS feed only gain optional fields.
8. Sync pushes before it pulls. Edits go up as the same small operations as undo; an operation whose `before` no longer matches becomes a conflict, kept on the computer, never a block. Every Convex write takes the next number from one sync clock, so a pull never misses a late commit.
9. Fields that only mean something on one computer (capture sources, slide images, file paths) stay in `device_binding` and never sync. Media files match by SHA-256.
10. Templates are a list of parts; a service made from one is plain items with a section per part, so changing a template never changes planned services.
11. A Bible passage is stored as a reference and translation ids, never as text, so licensed Bibles never leave the computer that imported them. Bibles are gzipped TSV, loaded on first use; the two bundled ones are compiled in.
12. Slides are fitted when the service resolves: text that would not fit at 70 % of the set size splits into more slides, measured with the projection's fonts at 16:9. The renderer still shrinks anything that overflows.
