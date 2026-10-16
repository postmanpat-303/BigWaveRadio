//! `bwr-plugin-check <plugin-folder>...`: validate BigWave Radio plugins
//! without the app (same checks as `BigWaveRadio.exe --check-plugin`).
//! Exit code 0 when every plugin passes. Used by the community plugin hub CI.

fn main() {
    let dirs: Vec<String> = std::env::args().skip(1).collect();
    if dirs.is_empty() || dirs.iter().any(|d| d == "-h" || d == "--help") {
        eprintln!("usage: bwr-plugin-check <plugin-folder>...");
        std::process::exit(2);
    }
    if dirs.iter().any(|d| d == "--version") {
        println!("bwr-plugin-check {} (plugin API {})", env!("CARGO_PKG_VERSION"), bwr_plugins::API_VERSION);
        return;
    }
    let mut failed = 0;
    for d in &dirs {
        let r = bwr_plugins::check_plugin(std::path::Path::new(d));
        println!("== {d}");
        print!("{r}");
        if !r.ok() {
            failed += 1;
        }
    }
    std::process::exit(if failed > 0 { 1 } else { 0 });
}
