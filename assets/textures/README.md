# Planet Textures

The app looks for the following files in this folder:

- `sun.jpg`
- `mercury.jpg`
- `venus.jpg`
- `earth.jpg`
- `moon.jpg`
- `mars.jpg`
- `jupiter.jpg`
- `saturn.jpg`
- `uranus.jpg`
- `neptune.jpg`

If files are missing, the app falls back to plain colors.

Additional optional textures:

- `milky_way_8k.jpg` (equirectangular sky texture, useful for a skybox/background sphere)
- `sun_2k_backup.jpg` (backup of the older lower-resolution sun map)
- `ceres.jpg` (Dawn Ceres mosaic)
- `vesta.jpg` (Dawn Vesta mosaic)
- `pluto.jpg` (New Horizons Pluto mosaic)
- `charon.jpg` (New Horizons Charon mosaic)

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

This script pulls textures from Solar System Scope public endpoints.
Before shipping or redistributing, verify current attribution/license terms.

Minor-body science textures:

```bash
# macOS / Linux
./scripts/download_textures_minor_bodies_science.sh
```

```powershell
# Windows
.\scripts\download_textures_minor_bodies_science.ps1
```

Heavy full-resolution source products (converted locally to JPEG):

```bash
# macOS / Linux
FULL_RES=1 TARGET_WIDTH=4096 ./scripts/download_textures_minor_bodies_science.sh
```

```powershell
# Windows
.\scripts\download_textures_minor_bodies_science.ps1 -FullRes -TargetWidth 4096
```

## GPU texture compression (optional)

After downloading, you can encode the planet/moon maps into GPU block-compressed
KTX2 (BC7 + mipmaps). The app automatically prefers a same-stem `.ktx2` (or
`.dds`) over the plain `.jpg` at load time, so this is a drop-in optimisation:

```bash
# macOS / Linux
./scripts/compress_textures.sh
```

```powershell
# Windows
.\scripts\compress_textures.ps1
```

This requires [AMD Compressonator](https://gpuopen.com/compressonator/) on PATH.
BC7 keeps textures block-compressed in VRAM (~4x smaller than the RGBA8 the
JPEGs decode to) and the embedded mip chain removes shimmer on small/distant
bodies. The 8K Milky Way backdrop is deliberately left as JPEG — its pixels are
read on the CPU to build the environment cubemap, which a compressed image can't
provide.

Both loaders read raw BCn with no Basis transcoder, so the portable build gains
no native dependency. If you would rather produce `.dds` with Microsoft's
`texconv` (`-f BC7_UNORM_SRGB -m 0`), that works too — the loader accepts either
container.

**macOS / Apple Silicon:** the compression scripts deliberately skip macOS —
Apple Silicon GPUs (Metal) cannot load BC7/BCn textures, so the app uses the
`.jpg` maps directly there. (Compressing for Apple Silicon would require an
ASTC or Basis Universal pipeline instead.)

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
