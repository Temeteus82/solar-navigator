# Solari raytracing on Metal — measurements (local experiment)

Branch `local/metal-raytracing-experiment`, `--features raytracing`.

## Setup

- Apple M5 (integrated), macOS 27.0, built-in Liquid Retina XDR display.
- Render size 2560×1440, release build, `SOLAR_NAVIGATOR_UNCAPPED=1`.
- Solari settings: `restir: true`, `world_cache_position_base_cell_size: 0.005`,
  the rest at Bevy 0.20 defaults (`raytracing.rs:solari_settings`).
- View: the startup view, i.e. the Sun from afar with no body selected, simulation
  paused. Most of the frame is sky, so this is a light case for Solari.

## Results (2026-10-23)

Measured with the Metal Performance HUD (`MTL_HUD_ENABLED=1`), read from screenshots
after ~25 s of warm-up and 10 s after pressing R.

| Lighting                         | GPU time / frame | FPS   | Metal memory |
| -------------------------------- | ---------------- | ----- | ------------ |
| Solari (R on)                    | 21.97 ms         | 44.9  | 1.72 GB      |
| Deferred raster (R off)          | 12.60 ms         | 60.0  | 1.33 GB      |

Solari adds about 9.4 ms of GPU time per frame (+75 %) and about 0.4 GB, and drops
below 60 fps on this view.

## What the in-app diagnostics can't show on macOS

- **Frame time stays at the display rate.** With `AutoNoVsync` the raster path still
  reported a steady 16.67 ms, so on macOS frame time shows only whether a frame misses
  the refresh, not the GPU cost. (Solari averaged 22.2 ms, the same figure the HUD
  gives.) Use the Metal HUD or an Xcode GPU capture for real GPU time.
- **No per-pass GPU timings.** `RenderDiagnosticsPlugin` logged only `elapsed_cpu`
  spans. Apple GPUs support timestamp sampling only at stage boundaries, so wgpu does
  not expose `TIMESTAMP_QUERY_INSIDE_ENCODERS`/`_PASSES`, which per-pass GPU spans need.
- `render/solari_lighting/world_cache_active_cells_count` read 0 throughout this view.
  It may just reflect a frame that is mostly sky; check it again on a planet close-up.

## Earth view, with and without ReSTIR (2026-10-23)

Same setup, but started with `SOLAR_NAVIGATOR_FOCUS=Earth` (the panel's normal
jump-to framing: Earth plus the Moon, Earth about 5 % of the frame). `T` toggles
ReSTIR at runtime. Read 30 s after launch and 12 s after each key press.

| Lighting                          | GPU time / frame | FPS  | Metal memory |
| --------------------------------- | ---------------- | ---- | ------------ |
| Solari, ReSTIR on                 | 22.48 ms         | 43.8 | 1.72 GB      |
| Solari, ReSTIR off                | 14.23 ms         | 60.0 | 1.29 GB      |
| Deferred raster (R off)           | 12.34 ms         | 60.0 | 1.33 GB      |

- **ReSTIR is nearly all of the cost.** Solari without it adds only ~1.9 ms over
  raster; ReSTIR adds another ~8.3 ms and ~0.4 GB.
- **The cost scales with resolution, not with what's on screen.** ReSTIR-on was
  22.0 ms on the near-empty startup view and 22.5 ms here. Its reservoirs are
  per-pixel: two 48-byte buffers at 2560×1440 come to ~0.35 GB, which matches the
  memory jump.
- **Without ReSTIR, the image has visible noise.** Earth is covered in scattered
  black pixels (dropouts where no light sample landed). The Moon and the raster
  image are clean. With ReSTIR on, Earth looks clean and close to the raster image.
  Without a denoiser on macOS, ReSTIR is what makes the output usable.

## Earth view after #105 (2026-10-23)

Same procedure as above, on `main` at `1653b57`, which adds #105: the sky's environment
map is filtered once and then frozen instead of re-filtered every frame. Solari samples
that `EnvironmentMapLight` on ray misses, so it was worth checking both paths.

| Lighting                | Before #105 | After #105 | Change  |
| ----------------------- | ----------- | ---------- | ------- |
| Solari, ReSTIR on       | 22.48 ms    | 18.36 ms   | −4.1 ms |
| Solari, ReSTIR off      | 14.23 ms    | 10.77 ms   | −3.5 ms |
| Deferred raster (R off) | 12.34 ms    | 11.39 ms   | −1.0 ms |

- **The image is unchanged.** The Earth crops with ReSTIR on and with raster lighting
  match the earlier ones (lighting, colour, the Moon). Metal memory is the same as
  before: 1.72 / 1.29 / 1.33 GB.
- **Solari gained more than raster.** 3.5–4 ms against 1 ms, so Solari without ReSTIR
  now reads slightly below raster. The cause is not known. Each figure is a single HUD
  reading, so differences of about a millisecond may be run-to-run variation.
- With ReSTIR on it is still below 60 fps (48 fps).

## Not yet measured

- A view where a planet fills the frame (zoomed in past the jump framing).
- Lower render resolution: since cost tracks pixel count, ReSTIR at reduced
  resolution plus upscaling may be the practical setting on Apple GPUs.
