# Hardware target and optimization plan

## Target computer

- CPU: AMD Ryzen 3 2200G.
- System memory: 8GB DDR4 at 2400MHz.
- GPU: NVIDIA GTX 1050 Ti with 4GB VRAM.

This is a design target, not a measured minimum specification. Static lyrics,
images, and simple GPU effects should fit this hardware. Video and screen capture
need implementation improvements and testing on the actual computer before live use.

## Display requirements

The resolutions below describe the initial test setup, not fixed application
requirements. Display roles must work with different resolutions, aspect ratios
(including 4:3, 16:9, and ultrawide), physical sizes, and DPI scaling.

| Display | Example resolution | Purpose |
| --- | --- | --- |
| Operator primary | 1920 × 1080 | Control panel |
| Main projection | 1920 × 1080 | Audience output |
| Confidence projector | 1280 × 720 | Stage output |
| Operator secondary | 1920 × 1080 | Additional operator workspace and screen capture source |

All four displays must operate simultaneously. Verify the exact graphics card's
ports and simultaneous display limit. If needed, investigate using the motherboard
video output through the 2200G's integrated GPU, subject to motherboard and BIOS
support. A mixed-GPU setup needs separate testing because frame transfers between
GPUs can add overhead.

Provide persistent monitor assignment and fullscreen behavior for the main and
confidence outputs. Handle disconnected displays without silently capturing or
projecting the wrong screen.

### Adaptive layout and scaling

- Detect each selected display's current pixel dimensions and scale factor. Update
  rendering when a window is resized, moved between displays, or its display mode
  or DPI changes. Do not hardcode a 1920 × 1080 or 1280 × 720 output canvas.
- Size each output independently to its actual viewport. Keep logical layout
  coordinates separate from physical render pixels to handle DPI scaling correctly.
- Use proportional positioning and configurable safe margins for lyrics and
  confidence content. Reflow text and adjust font size to fit narrower layouts
  without clipping; preserve operator-configurable minimum readable text sizes.
- Allow per-output font size, margins, and layout adjustments for physical screen
  size and viewing distance, which pixel resolution alone cannot determine.
- Default images, videos, and screen capture to **fit**: preserve source aspect
  ratio and letterbox or pillarbox where necessary. Provide an optional **fill**
  mode that preserves aspect ratio while cropping, with adjustable crop alignment.
  Stretching should only occur through an explicit user choice.
- Adapt shader coordinates to viewport aspect ratio so shapes are not distorted.
- Reconfigure surfaces and resolution-dependent resources on size changes,
  retaining shared source textures where possible. Suspend rendering for zero-size
  or minimized windows rather than creating invalid surfaces.
- Keep native output resolution separate from configurable capture and effect
  resolution budgets; a higher-resolution display must not force an equally large
  capture buffer or expensive shader target.

## Content requirements

- Continue supporting lyrics and static image backgrounds.
- Add light animated gradients and simple shader backgrounds, such as subtle
  waves or noise. Keep text readable over moving backgrounds.
- Support 1920 × 1080 video backgrounds at 30fps, including looping and text overlays.
- Capture the operator's secondary display at its detected dimensions, with a
  configurable resolution budget and frame rate. The initial performance target
  is up to 1920 × 1080 at 30fps; preserve aspect ratio when reducing larger or
  differently shaped sources to that budget.
- Route that capture to the main projection, confidence output, or both.
- When routed to both, acquire each capture frame once and reuse it for both outputs.
- Scale captured content independently to each destination viewport, preserving
  aspect ratio by default, including when source and destination shapes differ.
- Allow independent source selection for main and confidence outputs.

The initial performance target is one shared video or capture source with light
overlays. Separate moving sources on each output, or combinations of video,
capture, and animated effects, require additional measurement; they are not yet
validated on this hardware.

## Current implementation constraints

- `crates/core_engine/src/render.rs` uses wgpu for a simple background and text
  composition. This is a suitable starting point for lightweight shaders.
- `crates/app/src/main.rs` requests periodic rendering while video is active and
  refreshes static output every 250ms. Its 16ms active interval is a scheduling
  interval, not a guarantee of 60fps.
- `crates/core_engine/src/video.rs` decodes video with FFmpeg and converts it to
  RGBA on the CPU. Hardware decoding is not configured.
- Video pacing currently sleeps 30ms after each decoded frame, rather than using
  presentation timestamps. Decode time adds to that delay.
- Decoded frames use an unbounded queue, and each render drains it to obtain the
  latest frame. Rendering stalls can allow RAM usage to grow.
- `crates/core_engine/src/scene/video.rs` reuses its GPU texture when frame
  dimensions stay unchanged, but uploads CPU RGBA data for each new frame.
- Screen capture, animated background sources, and independent main/confidence
  rendering are not implemented. Monitor selection, precise video timing, and
  seeking are also unfinished in the roadmap.

## Optimization routes

### 1. Bound memory and fix video timing

- Replace the unbounded decoded-frame queue with a small bounded queue or latest
  frame slot. Define separate policies for timestamped video and low-latency capture.
- Schedule video using presentation timestamps and a monotonic playback clock.
  Drop late frames when needed instead of accumulating latency.
- Implement actual pause, seek, and loop behavior in the decoder, including clock
  resets and clearing stale queued frames.
- Stop and release obsolete decoding or capture sessions promptly when switching
  sources; avoid lingering threads and buffers.
- Reuse frame buffers where practical. Respect FFmpeg row stride when converting
  or uploading frames rather than assuming all frame data is tightly packed.

A tightly packed 1080p RGBA frame is about 7.9MiB. Uploading 30 such frames per
second represents about 237MiB/s of pixel payload before extra copies and overhead.
At 720p, a frame is about 3.5MiB. Small queues and fewer copies matter more than
preloading large amounts of decoded video into RAM.

### 2. Improve video decoding and uploads

- Investigate FFmpeg hardware decoding supported by the target OS, driver, GPU,
  and media codec. Keep a software fallback.
- Where supported, share decoded GPU surfaces with the renderer. Hardware decoding
  alone does not remove CPU transfers if decoded surfaces are downloaded and
  uploaded again.
- Consider uploading native YUV planes and performing color conversion in a shader
  to reduce CPU conversion work and upload volume.
- Preserve texture reuse and investigate reusable staging buffers if profiling
  shows upload allocation or synchronization costs.
- Establish a tested media profile, initially 1080p30 H.264 with representative
  bitrates. Test other codecs separately rather than assuming equal decode cost.

### 3. Add an efficient screen capture pipeline

- Choose the OS-native capture API after the deployment OS is established. Linux
  X11, Linux Wayland, and Windows need different integration and permission handling.
- Prefer GPU-backed capture and texture sharing where the capture API and wgpu
  backend permit it. Provide a CPU-copy fallback where necessary.
- Capture the secondary operator screen once at 30fps and share the latest frame
  with both output renderers.
- Keep capture buffering short: discard stale frames to favor low latency.
- Scale on the GPU to each output's actual viewport using its selected fit/fill
  mode; avoid a second capture session or CPU resize solely for another output.
- Detect capture-source resolution changes and update buffers and scaling without
  requiring an application restart.
- Measure cross-GPU transfer costs if the captured display and projection outputs
  are attached to different GPUs.
- Handle source disconnects and capture failures with an explicit fallback rather
  than leaving an unnoticed stale image on air.

### 4. Keep animated shaders lightweight

- Start with one fullscreen shader pass using time and a few uniforms.
- Avoid many noise octaves, large blur kernels, and repeated full-resolution passes.
- Cache shader pipelines and reuse resources; do not compile shaders per frame.
- Extend the background activity logic so shaders redraw while animated and stop
  consuming continuous rendering time when inactive.
- Offer a 30fps effect mode and, if necessary, lower-resolution effect rendering
  followed by GPU upscaling. Allow static fallback on slower systems.
- Use elapsed time for animation so motion remains consistent when frames are dropped.

### 5. Share resources across outputs

- Separate source acquisition from output composition: one video decoder or
  capture session can feed multiple outputs with different overlays and scaling.
- Prefer a shared GPU device and reusable source textures when surfaces and adapter
  compatibility allow it. Account for mixed-GPU configurations explicitly.
- Render each output at its detected viewport resolution. Keep optional lower
  effect resolutions configurable per output and upscale backgrounds on the GPU;
  render text at output resolution for clarity.
- Redraw static outputs on changes, resize, or recovery events, with a low-frequency
  refresh if needed. Render moving sources at their intended cadence.
- Avoid duplicate full-resolution previews in operator webviews. Use thumbnails or
  lower-rate previews if an operator preview is added.

### 6. Preserve RAM and CPU headroom

- Use release builds for deployment and performance measurements.
- Keep decoded-frame queues, image caches, and preview buffers bounded.
- Avoid loading an entire playlist's full-resolution media into memory.
- Limit frontend updates to relevant state changes; avoid broadcasting per-frame
  pixel data through Tauri events or WebSocket messages.
- Profile decoding, conversion, capture, and rendering before adding more threads.
- Test with the actual operator applications open on the secondary screen.

## Validation and priorities

Implement bounded buffering and correct playback timing first, followed by monitor
assignment and independent output routing. Then add simple shaders and one shared
capture source. Pursue hardware decoding and GPU surface sharing based on measured
bottlenecks and deployment OS support.

Test the release build on the target PC with all four displays connected:

| Scenario | What to verify |
| --- | --- |
| Lyrics and static images | Responsive controls; low idle CPU/GPU usage |
| Animated gradient with lyrics | Stable selected effect cadence; readable text |
| 1080p30 video loop | Correct playback speed; acceptable frame drops; clean loop |
| Capture to main only | Readable image; low and stable capture latency |
| Capture to confidence only | Correct aspect ratio and scaling to its actual viewport |
| Capture to both | One acquisition session; stable output on both displays |
| Different moving sources on each output | CPU/GPU headroom and bounded memory |
| Repeated source switches and long service | No growing queues, RAM usage, or lingering workers |
| Display disconnect/reconnect | Predictable routing and safe recovery |
| Mixed 4:3, 16:9, and ultrawide displays | Independent layouts; no clipped text or distorted content |
| Resize, resolution change, and mixed DPI | Surface recovery; correct layout and sharp text |
| Fit/fill with mismatched source and output shapes | Correct bars or cropping; preserved aspect ratio |
| Different physical screen sizes and viewing distances | Usable per-output text and margin settings |
| Capture-source resolution change | Updated capture dimensions and destination scaling |

Record process RAM, VRAM, CPU and GPU utilization, decode time, render time, dropped
frames, and capture latency. Run a full-service rehearsal, not just short clips.
Set acceptance thresholds from those measurements before claiming reliable live use.

If the optimized app still lacks headroom, consider 16GB RAM for multitasking and
memory pressure, or a CPU upgrade for software decoding. The 2400MHz RAM speed and
4GB VRAM are not established bottlenecks; hardware changes should follow profiling.
