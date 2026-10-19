# Feasibility: CUDA / ROCm compute for a modelled asteroid belt

Date: 2026-10-18. Context: Solar Navigator at `d87992c` (Bevy 0.20, wgpu 30.0.1).

## Verdict

Native CUDA and ROCm support are **feasible but the wrong tool for this app**.
Both are possible to wire in from Rust today, but they would cover only part of
the platform matrix, need a native runtime on the user's machine, and still
leave the asteroid positions on the wrong side of a GPU boundary from Bevy's
renderer. Everything a bigger, realer belt needs is already reachable through
**the wgpu device Bevy owns**: a compute shader that evaluates the orbits and
writes straight into the instance buffer the belt is drawn from. That path runs
on every platform the app supports (Vulkan, DX12, Metal), on the AMD dev
machine, with zero new dependencies.

The arithmetic is also not the bottleneck people assume. The project's own
Kepler solver costs 0.14–0.21 µs per asteroid per frame on one core; the real
ceiling at scale is **one Bevy entity per asteroid**, which has to go
regardless of where the maths runs.

Recommendation, in order:

1. Replace per-asteroid entities with a single instanced draw per mesh variant,
   fed from a CPU-filled instance buffer (parallel iteration via Bevy's task
   pool). No new dependencies. Comfortable to ~100 k asteroids.
2. Move the Kepler evaluation into a WESL compute shader on Bevy's device,
   with GPU frustum culling and an indirect draw. Scales to the full Minor
   Planet Center catalogue (~1.5 M objects) with the positions never leaving
   the GPU. Promote only the selected or nearby asteroids to ordinary f64
   `WorldPosition` entities so the floating-origin precision rules still hold
   where the camera can see the difference.
3. Only if a goal appears that genuinely needs native f64 or CUDA-only
   libraries (a long perturbed N-body integration run offline, say), add
   CubeCL and write that one kernel in Rust with wgpu as the default backend
   and CUDA/HIP as opt-in extras. Nothing in the belt-rendering problem needs
   this.

## What "modelling the asteroid belt as new objects" can mean

Two levels, with very different costs:

**A. Kinematic catalogue (recommended).** Load real orbital elements for
every known asteroid and propagate each on its own Keplerian ellipse, exactly
as the app already does for Ceres and Vesta in portable mode. Positions stay a
pure function of the simulation date, so date jumps, reverse time and the
SPICE/analytic split keep working unchanged. The visible structure of the real
belt (Kirkwood gaps, Hildas, Trojans, the inclined families) is already encoded
in the catalogue's elements, so this gives the scientific realism "for free".
Sources: Minor Planet Center `MPCORB.DAT` (about 1.5 M orbits; the MPC
counted 648 413 unnumbered objects as 42 % of the total in May 2026), Lowell
`astorb.dat`, or the JPL Small-Body Database API. The planetary kernels
committed in `assets/spice/` contain no asteroids, and the per-body Horizons
sync does not scale to a catalogue, so the catalogue file would be the source
of truth, fetched by a download script per the repo's "scripts are the only
source" rule.

**B. Dynamical N-body (not recommended as a frame-time feature).** Integrate
every asteroid under the Sun plus the eight planets. Asteroid-asteroid gravity
is negligible, so this is still embarrassingly parallel (N test particles ×
9 perturbers) and GPU-friendly. But it breaks the app's core invariant that a
position is a function of the date: a jump of 100 years means re-integrating
from the catalogue epoch. At a 1-day step that is 36 500 steps × 1.5 M
particles × 9 force terms ≈ 5 × 10¹¹ evaluations, seconds on a discrete GPU
and minutes on a CPU, per date jump, and it has to run forwards and backwards.
It also changes nothing visible that the catalogue does not already show over
the app's 1600–2200 window. If it is ever wanted it belongs in a background
"reintegrate" job, and even then the same compute shader on Bevy's device is
enough (see precision below).

## Measured CPU baseline

A standalone copy of `ephemeris::orbital_position_au` and `solve_kepler`
(five Newton iterations, f64), run over synthetic belt orbits with the same
element ranges as `asteroids.rs` (release build, best of 20 frames, work split
across freshly spawned scoped threads each frame).

On this session's 4-core 2.1 GHz Xeon container:

| Asteroids | 1 thread | 4 threads |
| --- | --- | --- |
| 3 000 (today) | 0.63 ms | — |
| 100 000 | 21.8 ms | 5.8 ms |
| 1 000 000 | 221 ms | 58.6 ms |

On the Linux dev desktop, an AMD Ryzen 9 5950X (16 cores / 32 threads, up to
5.1 GHz), measured 2026-10-19:

| Asteroids | 1 thread | 4 threads | 8 threads | 16 threads | 32 threads |
| --- | --- | --- | --- | --- | --- |
| 3 000 (today) | 0.46 ms | 0.15 ms | 0.15 ms | 0.22 ms | 0.39 ms |
| 100 000 | 14.2 ms | 3.8 ms | 2.0 ms | 1.15 ms | 1.03 ms |
| 1 000 000 | 134 ms | 38 ms | 19.6 ms | 10.0 ms | 7.9 ms |
| 1 500 000 (full catalogue) | 203 ms | 57 ms | 29 ms | 15.1 ms | 9.2 ms |

`-C target-cpu=native` changed nothing beyond noise. The 3 000 row gets slower
past 4 threads only because the benchmark spawns OS threads every frame; Bevy's
persistent task pool would not show that, so read the large rows.

One 5950X core evaluates an asteroid in ~0.14 µs, against ~0.21 µs on the Xeon.
With all 32 threads the full catalogue fits in a 60 fps frame (9.2 ms of
16.7 ms), but only by occupying every core for most of it, which leaves Bevy's
own systems and render preparation nothing. Going from 16 to 32 threads gains
only ~1.6×, because SMT siblings share the floating-point units. So the full
catalogue is not a practical per-frame CPU workload, even on a high-end desktop.
100 k is comfortable: 1–2 ms on 8 or more threads. The current 3 000 is noise.
`sin`/`cos` dominate, which is exactly what a GPU does cheaply: 1.5 M threads
of ~40 transcendental-light flops is well under a millisecond on anything
discrete.

The entity overhead is the larger cost and is not in that table. Each asteroid
today is a Bevy entity with `Transform`, `WorldPosition`, visibility and mesh
components, so every frame pays transform propagation, visibility resolution,
render-world extraction and GPU preprocessing per entity. That path is fine at
3 000 and strained well before 100 000. Both recommended steps remove it:
the belt becomes one entity per mesh variant with an instance buffer.

## Backend comparison

| | wgpu compute (Bevy's own device) | CUDA (`cudarc`) | ROCm / HIP (`cubecl-hip-sys`) | CubeCL (kernel in Rust, multi-backend) |
| --- | --- | --- | --- | --- |
| Windows x86_64 | Yes (Vulkan, DX12) | NVIDIA only | **No**: bindings are Linux-only, and the HIP SDK for Windows 7.x marks the gfx1030 parts (W6800, 6950 XT, same silicon as the dev RX 6900 XT) unsupported | Per backend |
| Linux x86_64 | Yes | NVIDIA only | AMD; gfx1030 is supported via PRO W6800/V620, the RX 6900 XT itself is unlisted and runs unvalidated | Per backend |
| macOS arm64 (a supported target) | Yes (Metal) | **No** | **No** | wgpu/Metal only |
| AMD RX 6900 XT dev box (Windows) | Yes | No | No | wgpu only |
| Build-time dependency | None (wgpu 30 already in the tree) | None (`dynamic-loading` dlopens `libcuda`), but kernels need PTX built ahead or NVRTC at runtime | None at build; ROCm runtime at run | Large crate tree; `cubecl-wgpu` 0.11.0 pins `wgpu-hal ^30`, matching Bevy 0.20 today but drifting on either side's next bump |
| Runtime dependency to ship | None | CUDA driver (+ NVRTC redistributable) | ROCm runtime | Per backend |
| Getting results to the renderer | Same device: the compute pass writes the instance buffer, zero copies | Read back over PCIe each frame (≈48 MB for 1.5 M instances, plus a sync stall), or Vulkan external-memory import through unsafe `wgpu-hal`, Vulkan backend only | Same as CUDA | wgpu runtime can register Bevy's existing device (`init_device` on a `WgpuSetup`), so zero-copy is possible on that backend only |
| f64 | WGSL has none; wgpu's `SHADER_F64` is Vulkan-only and "16 to 64 times slower" per its own docs | Native (1/16–1/32 rate on consumer parts) | Native (same caveat) | Per backend |
| Testable in CI (no GPU runners) | Possible with a software Vulkan driver (lavapipe) on Linux | No | No | wgpu backend only |
| Ecosystem state (Oct 2026) | Stable; Bevy 0.20 ships `compute_shader_game_of_life`, `gpu_readback`, `storage_buffer`, `custom_shader_instancing`, `compute_mesh` examples | `cudarc` 0.19.9, CUDA 11.4–13.3; Rust-CUDA (`rustc_codegen_nvvm`) rebooted 2025 but no release since 0.3 (2022) | `cubecl-hip-sys` 7.14 (Linux only); `rocmrc`/`rocm-rs` early stage | 0.11.0 released 2026-10-06; backends: wgpu, CUDA, HIP, Metal, CPU |

The decisive rows are macOS and the dev machine. The app supports Apple
Silicon, where neither CUDA nor ROCm exists, so a wgpu path would have to be
written anyway as the fallback. Once it exists, the CUDA/ROCm paths are pure
duplication that cover fewer machines and add a redistributable.

## Precision and the floating origin

The app keeps world positions in f64 and subtracts the camera-relative
`RenderOrigin` before rounding to f32 (see `render.rs:apply_render_origin`).
A GPU belt in f32 cannot match that everywhere, so the design has to say where
f32 is acceptable:

- f32 gives ~1e-7 relative precision, about 50 km at 3 AU, i.e. roughly 0.7 %
  of a belt rock's 0.012-unit (~7 200 km) visual radius. Sub-pixel until the
  camera is within a few rock radii.
- The mean anomaly must not be formed as `M0 + n·t` in plain f32: at 100 000×
  speed `t` passes 10⁹ s and the product's ulp becomes thousands of km. Form it
  with a split-precision product and reduce modulo 2π before the Kepler solve,
  or pass the date as a hi/lo f32 pair. This is standard and cheap.
- Rule for the hybrid: the selected asteroid, anything being tracked, and any
  rock closer than some multiple of its drawn radius to the camera is promoted
  to an ordinary f64 `WorldPosition` entity (the same path the named bodies
  use) and masked out of the GPU swarm. That also gives "new objects": a
  catalogue asteroid the user clicks becomes a proper selectable body with the
  panel's facts, while the other 1.5 M stay instances.
- True-scale mode makes belt rocks kilometre-sized, far below the f32 error,
  but they are also invisible beyond ~1 000 km, so the same promotion rule
  covers it.

Native f64 on CUDA/HIP would remove the hybrid rule, but at 1/16–1/32 rate and
only on a subset of machines; the hybrid is the better trade.

## What the wgpu route looks like in this codebase

- A `RenderGraph` node with a `ComputePipeline` (WESL, since Bevy 0.20 routes
  `.wgsl` to wgpu raw) that reads a static elements buffer (`a, e, i, Ω, ω, M0,
  n, scale, variant`: ~40 B per asteroid, ~60 MB for the catalogue) and the
  per-frame uniforms (date as hi/lo, render origin as hi/lo, frustum planes),
  and appends visible instances to a per-variant instance buffer with an
  atomic counter feeding `draw_indexed_indirect`.
- A custom phase item per mesh variant (the `custom_shader_instancing` /
  `custom_phase_item` examples) that draws those buffers with the belt's
  shared `StandardMaterial` look. Far instances can switch to points or
  billboards in the same shader for LOD.
- The sun-position and render-origin uniforms follow the existing pattern in
  `render.rs:sync_shader_sun_positions`.
- Plain storage buffers only, no binding arrays: the DX12 bindless page fault
  recorded in `CLAUDE.md` is a reminder to test this on the DX12 path before
  merging, but storage-buffer compute is the most trodden path on every wgpu
  backend.
- Keep `asteroids_enabled`, the deterministic seed for the procedural fallback
  (the catalogue download can be optional like the textures), and the existing
  tests; add a test that the WESL Kepler solve agrees with
  `orbital_position_au` on a sampled set of elements, run on lavapipe in CI if
  headless wgpu proves workable there.

## What CUDA/ROCm would cost on top

- Two more Cargo features (`cuda`, `rocm`) with runtime detection and a
  fallback, doubling the CI matrix that already runs two variants, with no CI
  machine able to execute the kernels.
- Packaging changes in `installer.nsi` and the `package_*` scripts to detect
  or ship the CUDA driver/NVRTC or ROCm runtime.
- An interop layer: either a per-frame PCIe read-back and re-upload, or an
  unsafe `wgpu-hal` external-memory import that works only when wgpu picked
  Vulkan (never on Metal, and not on the DX12 path the project also tests).
- A second kernel language (CUDA C++/PTX or HIP C++) next to WESL, or CubeCL
  to share one Rust source, which means a large dependency tree whose wgpu pin
  has to track Bevy's.
- On the AMD dev machine on Windows, no ROCm path at all in 7.x, so the only
  way to run the HIP backend locally would be a Linux boot.

## Sources

- CubeCL docs and release listing: https://docs.rs/crate/cubecl/latest , crates.io `cubecl-wgpu` 0.11.0 (2026-10-06) with `wgpu-hal ^30`, `cubecl-cuda` 0.11.0 on `cudarc 0.19.6`, `cubecl-hip-sys` 7.14 README ("Works only on Linux")
- cudarc: https://docs.rs/crate/cudarc/latest (CUDA 11.4–13.3, `dynamic-loading` default)
- Rust-CUDA reboot: https://www.phoronix.com/news/Rust-CUDA-Project-Reboot ; `rustc_codegen_nvvm` last release 0.3.0 (2022): https://lib.rs/crates/rustc_codegen_nvvm ; nvptx baseline SM 7.0 in Rust 1.97: https://blog.rust-lang.org/2026/05/01/nvptx-baseline-update/
- ROCm Windows HIP SDK 7.2 system requirements (gfx1030 W6800/6950 XT marked unsupported; "if a GPU is not listed on this table, it is not officially supported"): https://rocm.docs.amd.com/projects/install-on-windows/en/docs-7.2/reference/system-requirements.html
- ROCm Linux compatibility (V620 gfx1030 on Ubuntu 22.04/24.04): https://rocm.docs.amd.com/en/docs-7.0.1/compatibility/compatibility-matrix.html ; consumer-card tracker: https://gpucompat.com/rocm-consumer-gpus/
- Rust ROCm wrappers: https://docs.rs/crate/rocmrc/0.5.0 , https://docs.rs/crate/rocm-rs/0.4.0 , https://github.com/cjordan/hip-sys
- wgpu-types 30.0.1 `Features::SHADER_F64` doc comment (Vulkan only, 16–64× slower), read from the crates.io tarball
- Bevy 0.20 examples: https://github.com/bevyengine/bevy/tree/v0.20.0/examples/shader and `/examples/shader_advanced` ; https://bevy.org/examples/shaders/custom-shader-instancing/ ; https://bevy.org/examples/shaders/custom-phase-item/
- CUDA↔Vulkan interop from Rust: https://docs.rs/wgpu-cuda-interop ; HIP external interop: https://rocmdocs.amd.com/projects/HIP/en/latest/_sources/how-to/hip_runtime_api/external_interop.rst
- Minor planet counts: https://en.wikipedia.org/wiki/List_of_unnumbered_minor_planets ; https://asteroid.lowell.edu/astorb/
