# SPICE Kernels

Required kernel files for SPICE mode:

- `naif0012.tls` (leap seconds)
- `de440s.bsp` (planetary ephemerides)

Optional kernels loaded if present:

- `pck00011.tpc` (planetary constants)
- `gm_de440.tpc` (gravity constants from DE440)

Use this from project root to fetch the kernels:

```bash
# macOS / Linux
./scripts/download_spice_kernels.sh
```

```powershell
# Windows
.\scripts\download_spice_kernels.ps1
```

For redistribution and licensing context, see:

- `THIRD_PARTY_NOTICES.md`
- `ASSET_ATTRIBUTION.md`
