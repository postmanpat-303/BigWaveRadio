//! cargo run --release -p bwr-plugins --example visual_bench
use bwr_plugins::{VisualEngine, VisualParams, SCENES};
fn main() {
    let (w, h) = VisualEngine::size_for(16.0 / 9.0, 1.0);
    for s in SCENES {
        let mut e = VisualEngine::new(w, h);
        let t0 = std::time::Instant::now();
        for f in 0..240 {
            let p = VisualParams { scene: s.to_string(), time: f as f32 / 60.0, zoom: 1.02, rotate: 0.01, warp: 0.5, decay: 0.92, level: 0.5, bass: 0.5, bands: vec![0.5; 32], ..Default::default() };
            e.render(&p);
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0 / 240.0;
        println!("{s:8} {w}x{h}: {ms:.2} ms/frame ({:.0} fps)", 1000.0 / ms);
    }
}
