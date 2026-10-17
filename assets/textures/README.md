# Planet Textures

**Nothing in this folder is stored in git** except this README — the folder is gitignored,
so a fresh checkout starts empty and the download scripts below are the only way to fill
it. Bodies whose texture is missing render in a flat fallback colour (and a missing
`milky_way_8k.png` also logs a warning at startup).

The app looks for the following files in this folder:

- `sun.png`
- `mercury.png`
- `venus.png` (plus `venus_clouds.png`)
- `earth.png`
- `moon.png`
- `mars.png`
- `jupiter.png`
- `saturn.png`
- `uranus.png`
- `neptune.png`

If files are missing, the app falls back to plain colors.

Additional optional textures:

- `milky_way_8k.png` (equirectangular sky texture, useful for a skybox/background sphere)
- `ceres.png` (Dawn Ceres mosaic)
- `vesta.png` (Dawn Vesta mosaic)
- `pluto.png` (New Horizons Pluto mosaic)
- `charon.png` (New Horizons Charon mosaic)
- `io.png`, `europa.png`, `ganymede.png`, `callisto.png` (Galilean moon mosaics;
  Europa and Callisto are greyscale — USGS publishes no colour map of either)

## Auto-download helper

From project root:

```bash
# macOS / Linux
./scripts/download_textures_solar_system_scope.sh
```

```powershell
# Windows
.\scripts\download_textures_solar_system_scope.ps1
```

This script pulls textures from Solar System Scope public endpoints — every file in the
first list above, including `venus_clouds.png` (from their `4k_venus_atmosphere` product;
there is no 8K one) and `milky_way_8k.png`. Before shipping or redistributing, verify
current attribution/license terms.

Minor-body science textures:

```bash
# macOS / Linux
./scripts/download_textures_minor_bodies_science.sh
```

```powershell
# Windows
.\scripts\download_textures_minor_bodies_science.ps1
```

That covers every minor body except **Vesta**: the only equirectangular Vesta mosaic
published anywhere is a 357 MB DLR product, so `vesta.png` is fetched in full-resolution
mode only and Vesta renders in its fallback colour until you run it. (The compact Vesta
product DLR publishes is Mollweide-projected and maps onto a sphere wrong, which is why the
script refuses to use it.)

Heavy full-resolution source products (converted locally to lossless PNG):

```bash
# macOS / Linux
FULL_RES=1 TARGET_WIDTH=4096 ./scripts/download_textures_minor_bodies_science.sh
```

```powershell
# Windows
.\scripts\download_textures_minor_bodies_science.ps1 -FullRes -TargetWidth 4096
```

## GPU texture compression (optional)

After downloading, you can encode the planet/moon maps into GPU block-compressed,
mipmapped KTX2. The app automatically prefers a same-stem `.ktx2` (or `.dds`)
over the plain `.png` at load time, so this is a drop-in optimisation:

```bash
cargo run --release --example compress_textures --features compress_textures
```

Pass `-- --force` to re-encode files that already have a `.ktx2`. The example
picks the GPU format your platform supports — **BC7 on Windows/Linux, ASTC 4x4
on macOS / Apple Silicon** (Metal supports ASTC, not BC7) — and encodes in-process
with [`ctt`](https://github.com/cwfitzgerald/ctt), the library behind Bevy 0.20's
`CompressedImageSaver`. No external encoder is needed, but `ctt` compiles its C++
encoders, so the `compress_textures` feature needs a C++ compiler (MSVC on
Windows, the Xcode Command Line Tools on macOS, `g++`/`clang++` on Linux). The app
builds themselves never enable it.

Either format keeps textures block-compressed in VRAM (~4x smaller than the
RGBA8 the source maps decode to) and the embedded mip chain removes shimmer on
small/distant bodies. The 8K Milky Way backdrop is deliberately left
uncompressed — its pixels are read on the CPU to build the environment cubemap,
which a compressed image can't provide.

The loaders read raw BCn/ASTC with no Basis transcoder, so the app gains no
native dependency. If you would rather produce `.dds` with Microsoft's `texconv`
(`-f BC7_UNORM_SRGB -m 0`), that works too — the loader accepts either container.

ASTC files load only on Apple Silicon (and mobile), and BC7 only on desktop, so
each platform keeps its own `.ktx2` set — fine here because textures are
generated locally per machine, never shared or committed. A single asset that
works everywhere would need Basis Universal (UASTC) transcoding, which this
project avoids for its C++ build dependency.

## Attribution

Current texture downloads in this project are sourced from Solar System Scope:

- https://www.solarsystemscope.com/textures/
- License listed on that page: CC BY 4.0

If you redistribute binaries or assets, include proper attribution for used textures.
See also:

- `ASSET_ATTRIBUTION.md`
- `THIRD_PARTY_NOTICES.md`

Minor-body texture sources used by the new helper script:

- DLR Dawn GIS Ceres/Vesta mosaic products:
  - https://dawngis.dlr.de/data/Ceres/mosaic_ceres.php
  - https://dawngis.dlr.de/data/Vesta/mosaic_vesta.php
- USGS Astrogeology / New Horizons Pluto and Charon products:
  - https://astrogeology.usgs.gov/search/map/pluto_new_horizons_lorri_mvic_global_mosaic_300m
  - https://astrogeology.usgs.gov/search/map/charon_new_horizons_lorri_mvic_global_mosaic_300m
