fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Point winresource at the llvm-mingw windres so no PATH setup is needed.
        let windres = "C:/Users/DMAN/tools/llvm-mingw-20261006-ucrt-x86_64/bin/x86_64-w64-mingw32-windres.exe";
        if std::path::Path::new(windres).exists() {
            // SAFETY: build script, single-threaded, before res.compile().
            unsafe { std::env::set_var("WINDRES", windres) };
        }
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/bigwave.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon embedding skipped: {e}");
        }
    }
}
