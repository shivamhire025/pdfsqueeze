// Windows only: embed assets/icon.ico in the executable, so the file, its taskbar button and
// pinned shortcuts show the app icon (the window's own icon is set at run time in src/ui.rs).
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "pdfsqueeze");
        res.set("FileDescription", "pdfsqueeze: offline PDF toolkit");
        if let Err(e) = res.compile() {
            // A missing resource compiler must not stop the build; the window still gets its icon.
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
