# GPU Profiling & Debugging

These are **external developer tools you install separately** — not crate
dependencies. They are the practical way to verify the GPU-compressed texture
pipeline (BC7 / ASTC) actually lands on the GPU and to profile the rendering
cost of the post-processing stack and the asteroid swarm.

> Build with `--release` before capturing. Debug builds are not representative
> of real frame timings or memory.

## Pick a capturable backend

Solar Navigator renders through [wgpu](https://wgpu.rs/) (via Bevy), which
selects a backend per platform: **D3D12** on Windows, **Vulkan** on Linux,
**Metal** on macOS. The desktop capture tools below all support **Vulkan**, so
forcing Vulkan is the common denominator on Windows/Linux:

```bash
# macOS / Linux
WGPU_BACKEND=vulkan cargo run --release
```

```powershell
# Windows
$env:WGPU_BACKEND = 'vulkan'; cargo run --release
```

Valid values: `vulkan`, `dx12`, `metal`, `gl`. On **macOS** keep the default
`metal` — RenderDoc/RGP/Nsight do not support Metal; use Xcode instead (below).

## RenderDoc — frame debugger (Windows/Linux)

Best single tool for *correctness*: inspect every resource, pass, and shader in
a captured frame. Install from <https://renderdoc.org/>.

1. **Executable path:** `target/release/solar-navigator` (`.exe` on Windows).
2. **Working directory:** the repo root, so `assets/` resolves.
3. **Environment variable:** add `WGPU_BACKEND=vulkan`.
4. Launch, then capture a frame (default `F12`).

What to verify for this project:

- **Texture Viewer →** pick a planet base-color texture and confirm the
  **Format** is a block format (`BC7_SRGB...` on desktop, `ASTC...` on Apple
  Silicon) **with a full mip chain** — not a single-level `RGBA8` 2048×1024.
  This is the on-GPU proof that `scripts/compress_textures.*` worked and that
  the `.ktx2` was preferred over the `.jpg`.
- **Event Browser →** Bevy labels its render passes, so you can step through the
  depth/normal prepass, SSAO, the custom **atmosphere** limb-glow (additive,
  front-face-culled, no depth write) and **Saturn ring** (umbra) draws, bloom,
  and auto-exposure.
- Select a draw to inspect bound resources, the WGSL shader, and pixel history —
  useful when debugging `planet_atmosphere.wgsl` / `planet_ring.wgsl`.

## AMD — Radeon GPU Profiler (RGP) + Radeon Memory Visualizer (RMV)

Install the Radeon Developer Tool Suite (Radeon Developer Panel + RGP + RMV)
from <https://gpuopen.com/tools/>. Run the app through the Radeon Developer
Panel with the Vulkan backend.

- **RGP** (timing): capture a profile to see per-pass GPU time. Watch the
  post-FX stack (SSAO + prepasses + bloom + auto-exposure) and the per-frame
  cost of the ~3,000 instanced asteroids.
- **RMV** (memory): capture a memory trace to see actual VRAM by resource. Run
  it **before and after** `scripts/compress_textures.*` to quantify the texture
  VRAM reduction (the ~4× BC7/ASTC claim) against the uncompressed RGBA8 the
  JPEGs decode to.

## NVIDIA — Nsight Graphics

Install from <https://developer.nvidia.com/nsight-graphics>. Launch the app
(Vulkan backend) under Nsight: **GPU Trace** for per-pass timings and **Frame
Debugger** for resource/texture inspection — the NVIDIA equivalent of
RenderDoc + RGP combined.

## macOS / Apple Silicon — Xcode

RenderDoc, RGP, and Nsight do not support Metal. Use Xcode's Metal tooling
instead (default `metal` backend):

- **Xcode → Debug → Capture GPU Frame** (attach to the running process) to
  inspect passes, resources, and confirm the **ASTC** texture format + mip
  chain.
- **Instruments → Metal System Trace** for GPU timeline/timings.

## CPU-side work (not visible to GPU tools)

The asteroid Keplerian update, body positioning, and Horizons sync are CPU work
and will not show up in the GPU profilers above. For those, profile with
[`cargo flamegraph`](https://github.com/flamegraph-rs/flamegraph) or enable
Bevy's Tracy integration (`bevy/trace_tracy`) and capture with
[Tracy](https://github.com/wolfpld/tracy).

## If you find a GPU bottleneck

Likely culprits on weaker hardware are the post-processing passes. Candidates to
expose behind a future "minimal effects" toggle: SSAO + the depth/normal
prepasses that feed it, bloom intensity, and the asteroid count
(`ASTEROID_COUNT` in `src/app/asteroids.rs`).
