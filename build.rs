fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/AppIcon.ico");
        res.compile().expect("failed to compile Windows resources");
    }
}
