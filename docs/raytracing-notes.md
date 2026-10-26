# Solari raytracing — measurements (experimental)

Opt-in `--features raytracing` (`src/app/raytracing.rs`), off in both app builds.
Measured on Metal (Apple M5) and on Vulkan (AMD RX 6900 XT, Linux) below.

## Metal: setup

- Apple M5 (integrated), macOS 27.0, built-in Liquid Retina XDR display.
- Render size 2560×1440, release build, `SOLAR_NAVIGATOR_UNCAPPED=1`.
- Solari settings: `restir: true`, `world_cache_position_base_cell_size: 0.005`,
  the rest at Bevy 0.20 defaults (`raytracing.rs:solari_settings`).
- View: the startup view, i.e. the Sun from afar with no body selected, simulation
  paused. Most of the frame is sky, so this is a light case for Solari.

## Metal: startup view (2026-10-23)

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

## Metal: Earth view, with and without ReSTIR (2026-10-23)

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

## Metal: Earth view after #105 (2026-10-23)

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

## Vulkan on Linux: Earth view (2026-10-25)

- AMD Radeon RX 6900 XT, RADV (Mesa 26.2.5), CachyOS, `main` at `da73674` (after #105).
- 1280×720 window, release SPICE build, the 60 fps cap left **on**: CLAUDE.md records an
  uncapped run hard-freezing this GPU under Windows, and Vulkan's per-pass GPU timings
  give the cost without it.
- `SOLAR_NAVIGATOR_FOCUS=Earth`, simulation paused, default (non-true-scale) bodies.
  Solari settings as on Metal.
- No key injection tool was available, so a temporary local patch picked the starting
  mode instead of `R`/`T`. Starting without `SolariLighting` also needs the deferred,
  depth and motion-vector prepasses that `SolariLighting` requires, which the `R`-off
  state keeps; without them Bevy panics in `prepass/mod.rs`.
- Timings are `RenderDiagnosticsPlugin` averages from the last 5 s report, after ~35 s.
  The totals sum the instrumented GPU passes, without the one-off `blas_build` startup
  spike and the environment-map filter passes, which run twice at startup and then stop
  (#105). They are not a full frame: work outside the instrumented passes is missing.

| Lighting                | Solari lighting pass | Sum of GPU passes |
| ----------------------- | -------------------- | ----------------- |
| Solari, ReSTIR on       | 2.70 ms              | 5.57 ms           |
| Solari, ReSTIR off      | 1.17 ms              | 4.04 ms           |
| Deferred raster (R off) | — (`deferred_lighting` 0.53 ms) | 3.38 ms |

Other Solari passes, in both Solari modes: TLAS build 0.55 ms, world cache 0.17–0.18 ms,
light-tile presampling 0.03 ms.

- **Same picture as on Metal.** ReSTIR is most of Solari's cost (+1.5 ms here); without it
  Earth shows scattered black dropouts, while with it the image is close to raster.
- **The TLAS is rebuilt in raster mode too** (0.55 ms with Solari off): Solari's scene
  plugin keeps building the acceleration structure whenever `RaytracingMesh3d`s exist,
  whether or not a camera has `SolariLighting`.
- **SSAO keeps running under Solari** (0.84 ms here). Solari's GI makes it redundant, so
  skipping it while Solari is on would save that.
- At a larger window (render area about 1580×1025, ReSTIR on) the lighting pass was
  5.5 ms in an earlier run, consistent with the per-pixel cost seen on Metal.

### GPU state during the runs (`amdgpu_top`, 2026-10-26)

Repeated the three runs with `amdgpu_top -J -s 500` logging alongside the app (same
setup; Bevy's timings reproduced the table above within 0.05 ms). Means over the same last
10 s as the diagnostics report; "app" columns are our process from amdgpu_top's per-process
(fdinfo) data.

| Lighting                | Shader clock | GFX busy (GPU / app) | Power | App VRAM |
| ----------------------- | ------------ | -------------------- | ----- | -------- |
| Idle, no app            | ~0 MHz       | 0 % / —              | 30 W  | — (desktop: 666 MiB) |
| Solari, ReSTIR on       | 479 MHz      | 41 % / 41 %          | 35 W  | 1508 MiB |
| Solari, ReSTIR off      | 351 MHz      | 26 % / 31 %          | 32 W  | 1524 MiB |
| Deferred raster (R off) | 480 MHz      | 30 % / 27 %          | 31 W  | 1606 MiB |

The memory clock stayed at its 1000 MHz maximum throughout, idle included; junction
temperature stayed at 38–39 °C.

- **The timings above were measured at the lowest shader clock.** This card's shader
  clock range is 500–2720 MHz (`pp_dpm_sclk`, power level `auto`). Under the 60 fps cap
  the load is light enough that the driver keeps the GPU in its lowest power state
  (about 500 MHz, briefly lower), roughly 18 % of the maximum. All three modes ran at that
  clock, so the comparison between them holds, but the absolute milliseconds overstate
  what the passes cost on a fully clocked GPU. Shader-bound work would shrink the most;
  memory-bound work, already at full memory clock, would not.
- **No ReSTIR memory jump at this size.** App VRAM was 1.5–1.6 GB in all three modes,
  and the differences (ReSTIR on was the *lowest*) look like run-to-run variation rather
  than a lighting-mode effect. On Metal, ReSTIR added ~0.4 GB at 2560×1440; scaled to
  this pixel count that would be ~0.1 GB, within the spread seen here.
- **GFX busy % is not a cost measure here.** It is busy time at whatever clock the GPU
  picked, and the clock varied between runs (the ReSTIR-off run averaged lower, with
  dips to 65 MHz), so busy % doesn't rank the modes consistently. Use the per-pass
  timings for comparisons, at a fixed clock if possible.

## Not yet measured

- A view where a planet fills the frame (zoomed in past the jump framing).
- Lower render resolution: since cost tracks pixel count, ReSTIR at reduced
  resolution plus upscaling may be the practical setting on Apple GPUs.
- Vulkan at 2560×1440 uncapped, for a direct comparison with the Metal tables.
- Vulkan with the shader clock pinned high (`power_dpm_force_performance_level` =
  `high`, needs root) for full-clock per-pass costs.
- DX12 on Windows: Solari needs binding arrays, the feature family behind the DX12
  bindless crash recorded in CLAUDE.md.
