#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

// macOS support is Apple Silicon (aarch64) only. Apple has stopped shipping new
// Intel Macs, the CSPICE toolkit is vendored for arm64, and the GPU texture
// pipeline assumes Apple Silicon's Metal feature set (e.g. ASTC, not BC7).
// Refuse to build the unsupported Intel/x86_64 macOS target up front rather
// than fail in a confusing way later.
#[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
compile_error!(
    "Solar Navigator on macOS is supported on Apple Silicon (aarch64) only; \
     Intel/x86_64 Macs are not supported."
);

mod app;
mod ephemeris;

fn main() {
    app::run();
}
