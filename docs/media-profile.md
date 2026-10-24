# Supported media

What PALTO plays, how each kind of file is handled, and what has been measured. The design is in [ENGINE.md](../ENGINE.md#media); the church PC numbers come from `bun desktop/tools/e2e/profile.ts` runs (see the [checklist](church-pc-checklist.md)).

## Recommended for services

Export or convert videos to this when you can. It is the cheapest to play and the best tested:

| | |
|---|---|
| Container | MP4 |
| Video | H.264 (High profile), 8-bit 4:2:0 (`yuv420p`), 1920×1080 or smaller, 24–60 fps, 5–20 Mbit/s |
| Audio | AAC, 44.1 or 48 kHz, stereo |
| Loops (backgrounds) | The same, muted or without audio. Start and end on matching frames; the loop point is prepared, so no seek shows. |

HEVC and VP9 at 1080p play just as well and decode in hardware on most GPUs. Larger files (4K) play, but cost more to decode and to copy; scale them down to 1080p for the projector if they will be used often.

## How files are handled

| Kind | Examples | Path |
|---|---|---|
| 8-bit 4:2:0 up to 4096×2304 | H.264, HEVC, VP9, AV1, MPEG-2 | Planes uploaded as decoded; converted and scaled on the GPU |
| Other YUV (4:2:2, 4:4:4, 10-bit) | ProRes 422, 10-bit HEVC, 4:2:2 H.264 | Converted to 8-bit 4:2:0 on the CPU, then as above |
| With transparency | ProRes 4444 with alpha, PNG sequences in MOV | Converted to RGBA on the CPU (heaviest: about 10 ms per 1080p frame on the 5500U) |
| Larger than 4096×2304 | 6K, 8K | Scaled down to 1920×1080 on the CPU |
| Audio only | MP3, AAC, WAV, FLAC, Opus | Resampled to the output device |

FFmpeg 8.1 (the LGPL build shipped in the Windows installer) decodes everything its native decoders support. Hardware decoding, when chosen in Settings → System → Video decoding, applies to codecs the accelerator supports; other files decode in software in the same way as before.

## Tested here (development laptop)

AMD Ryzen 5 5500U (Radeon integrated, VA-API), Linux, 2026-10-24. Each clip played for 4 s as a loop; every clip showed every frame with none late, in software and with Automatic hardware decoding. "Copy back" is the per-frame cost of bringing a hardware-decoded frame to system memory.

| Clip | Software | Automatic hardware | Copy back (ms) |
|---|---|---|---|
| H.264 1080p30 and 1080p60, 4:2:0 | I420 | VA-API (NV12) | 5.8 |
| H.264 2160p30 (4K), 4:2:0 | I420 (GPU scales to 1080p) | VA-API (NV12) | — |
| H.264 720p, 4:2:2 | converted | software (VA-API has no 4:2:2) | — |
| HEVC 1080p, 8-bit | I420 | VA-API (NV12) | 7.4 |
| HEVC 720p, 10-bit | converted | converted (whether VA-API decoded it was not recorded) | — |
| VP9 1080p | I420 | VA-API (NV12) | 8.6 |
| AV1 1080p | I420 | software (no AV1 decoder on this GPU) | — |
| MPEG-2 1080p | I420 | VA-API (NV12) | 12.9 |
| MPEG-4 Part 2 640×360 | I420 | software (not supported by VA-API) | — |
| ProRes 422 1080p | converted (3 ms) | Vulkan, converted | 1.6 |
| ProRes 4444 with alpha 1080p | RGBA (10 ms) | Vulkan, RGBA | 3.3 |

A 1080p30 H.264 video looping with sound, with the confidence output presenting, used 25% of one CPU thread in software and 20% with VA-API (`profile.ts --compare`). Colors of SMPTE bars encoded as BT.709 limited range matched the source within 2 levels in both modes.

## Church PC (Ryzen 3 2200G, GTX 1050 Ti, Windows)

Not measured yet. Fill this in from the `profile-*.md` reports of the next visit:

| Video | Decoder | CPU (% of one thread) | Projection fps | Confidence fps | Transfer ms | Notes |
|---|---|---|---|---|---|---|
| | Software | | | | — | |
| | Automatic | | | | | |
