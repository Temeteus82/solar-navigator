fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/AppIcon.ico");
        res.compile().expect("failed to compile Windows resources");
    }

    // NAIF's prebuilt cspice.lib is compiled /MT and asks for the static CRT
    // (LIBCMT), while Rust links the DLL CRT; drop LIBCMT to stop LNK4098.
    if std::env::var_os("CARGO_FEATURE_SPICE").is_some()
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg=/NODEFAULTLIB:libcmt");
    }
}
