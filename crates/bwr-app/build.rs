//! Windows resources for BigWaveRadio.exe: the wolf icon, version info
//! (Del (postmanpat) / BigWave Radio) and the application manifest (asInvoker,
//! Windows 10/11, PerMonitorV2 DPI, Common Controls v6).
//! Release builds use MSVC (x86_64-pc-windows-msvc, see
//! .github/workflows/windows.yml): winresource finds rc.exe in the Windows SDK.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/bigwave.ico");
    println!("cargo:rerun-if-changed=bigwave.exe.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if !msvc {
        // Local llvm-mingw (gnullvm) builds: point winresource at that windres.
        let windres = "C:/Users/DMAN/tools/llvm-mingw-20261006-ucrt-x86_64/bin/x86_64-w64-mingw32-windres.exe";
        if std::path::Path::new(windres).exists() {
            // SAFETY: build script, single-threaded, before res.compile().
            unsafe { std::env::set_var("WINDRES", windres) };
        }
    }
    let ver = std::env::var("CARGO_PKG_VERSION").unwrap();
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/bigwave.ico")
        .set_manifest_file("bigwave.exe.manifest")
        .set("CompanyName", "Del (postmanpat)")
        .set("ProductName", "BigWave Radio")
        .set("FileDescription", "BigWave Radio")
        .set("InternalName", "BigWaveRadio")
        .set("OriginalFilename", "BigWaveRadio.exe")
        .set("LegalCopyright", "MIT, (c) 2026 Del (postmanpat)")
        .set("ProductVersion", &ver)
        .set("FileVersion", &ver);
    if let Err(e) = res.compile() {
        if msvc {
            // Release builds must carry the icon + manifest: fail loudly.
            panic!("embedding Windows resources failed: {e}");
        }
        println!("cargo:warning=icon embedding skipped: {e}");
    }
}
