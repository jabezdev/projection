# Project MVP Progress

## Main Screen
- [x] Configurable lyric fade, custom image/video logo, and operator monitor routing controls.
- [x] Render projection text in the projection webview; disable native WGPU text.
- [x] Add live, persistent output settings for fonts, size, weight, bold/italic, capitals, color, alignment, placement and safe margins.
- [x] Add outlines, shadows, glow, square borders and translucent text boxes.
- [x] Share the projection compositor with the operator preview.
- [x] Display local image/video backgrounds with fill/fit options and configurable fades.
- [x] Move media compositing into the webview for synchronized playback; the webview exclusively owns the output window, with no competing native swapchain.

## Operator Screen & Session Data
- [x] Resize columns through animation-frame CSS updates; memoize song layouts and keep nearby cues in the operator confidence preview.
- [x] Three resizable columns with a capped right column.
- [x] Continuous lyrics, numbered slides, colored part rails and direct cue selection.
- [x] Collapsible songs, arrangement cards, drag-and-drop and explicit quick edits.
- [x] Local SQLite songs, media library and service creation/editing.
- [x] Song author/singer, blank-line slide splitting, selectable split previews and projection previews.
- [x] Settings at the top of the left column; video transport on center cards, saved per-video clip/loop/volume/speed/confidence options with audio controls inside each center video card.

## Confidence Monitor
- [x] Continuous teleprompter that centers the active slide.
- [x] Colored vertical part containers with moving/sticky V1/C1 labels.
- [x] HH:MM clock and moving arrangement tape with desaturated previous parts.
- [x] Matching operator confidence preview and adjustable confidence text size.

## Media & MVP Presentations
- [x] Loopback HTTP video streaming with byte ranges and generated poster thumbnails.
- [x] Muted, looping, screen-filling song video backgrounds without transport controls.
- [x] Native file picker and local image/video/audio library.
- [x] Media cues inside the service schedule.
- [x] Projector owns video decoding and audio; operator preview uses thumbnails and sends transport commands.
- [x] Control projector-owned video seek, pause, speed, volume and looping without periodic follower seeks.
- [x] Import groups of images as presentations, reorder/remove images, save them in services and cue individual slides.
- [x] Edit existing image presentations; navigate slides through engine commands.
- [x] Standalone audio plays in the native media engine with play/pause, stop, seek, volume, mute, loop, clip and speed in its center card; the operator UI plays no audio or video.
- [x] Native output engine is the only output engine; the webview outputs were removed (2026-10-11). Previews are still images rendered by the output engine.
- [x] Video thumbnails decoded in-process (no FFmpeg executable needed).
- [x] Audio output reopens on the default device after a device failure.
- [ ] PowerPoint/PDF conversion (outside the image-group MVP).
- [ ] Package codec support consistently across target platforms: outputs decode with the bundled FFmpeg libraries.

## Release Verification & Packaging
- [x] Tauri Windows/Linux bundle targets configured (platform files bundle FFmpeg on Windows and declare it on Debian).
- [ ] Verify video audio, asset access and output windows on a packaged Linux build and Windows build.
- [ ] Test monitor routing, fullscreen behavior and hardware audio output with a real projection/confidence setup.
- [ ] Build and verify installers for both platforms (Windows steps in desktop/README.md, not yet run).

- [x] Relative 30/45/35 operator columns, previews fitted to actual output aspect ratios, left-cue navigation with scroll space, and selection-free dragging.

- [x] Persist output display identity and fullscreen mode per screen; preload and gently synchronize confidence video with readiness-based fades.
