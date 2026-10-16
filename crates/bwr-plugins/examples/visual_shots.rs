//! Render each scene offline to /tmp/vs-<scene>.ppm (for tuning).
use bwr_plugins::{Rgba, VisualEngine, VisualParams, SCENES};
fn main() {
    let presets: [(&str, f32, f32, f32, f32, f32, &[&str]); 8] = [
        ("tunnel", 1.035, 0.004, 0.3, 0.90, 1.0, &["#0b0420", "#3a0ca3", "#f72585", "#ffb703", "#4cc9f0"]),
        ("kaleido", 1.015, 0.012, 0.5, 0.93, 1.2, &["#ff006e", "#fb5607", "#ffbe0b", "#3a86ff", "#8338ec", "#06d6a0"]),
        ("plasma", 1.0, 0.0, 0.2, 0.86, 1.0, &["#240046", "#7b2cbf", "#e0aaff", "#00f5d4", "#fee440", "#f15bb5"]),
        ("flow", 1.002, 0.0, 0.1, 0.965, 1.0, &["#003300", "#00ff66", "#ccff00", "#00cc88", "#66ffcc"]),
        ("sphere", 1.012, -0.003, 0.0, 0.86, 1.0, &["#00f0ff", "#0077ff", "#ffffff", "#7b2ff7", "#00ffa3"]),
        ("terrain", 1.0, 0.0, 0.0, 0.55, 1.0, &["#2b00ff", "#00e5ff", "#00ff85", "#ffe600", "#ff2e63"]),
        ("stars", 1.0, 0.0, 0.0, 0.78, 1.0, &["#ffffff", "#9bd1ff", "#ffd6f6", "#b5fffc"]),
        ("mandala", 1.02, 0.006, 0.25, 0.92, 3.0, &["#ff00c8", "#ffe600", "#00ffd5", "#7a00ff", "#ff6b00"]),
    ];
    let only = std::env::args().nth(1);
    for (s, zoom, rotate, warp, decay, detail, pal) in presets {
        if only.as_deref().is_some_and(|o| o != s) { continue; }
        assert!(SCENES.contains(&s));
        let (w, h) = VisualEngine::size_for(16.0 / 9.0, 1.0);
        let mut e = VisualEngine::new(w, h);
        let mut px = Vec::new();
        for f in 0..300 {
            let t = f as f32 / 60.0;
            let ph = (t % 0.484) / 0.484;
            let kick = (-ph * 6.0).exp();
            let bands: Vec<f32> = (0..32).map(|i| { let x = i as f32 / 32.0; ((1.0 - x).powi(2) * kick * 0.9 + (0.5 + 0.5 * (t * (1.3 + x * 3.0) + x * 9.0).sin()) * 0.4 * (0.4 + 0.6 * x)).min(1.0) }).collect();
            let p = VisualParams { scene: s.into(), time: t, zoom, rotate, warp, decay, detail, palette: pal.iter().filter_map(|c| Rgba::parse(c)).collect(), level: 0.5, bass: kick * 0.8, mid: 0.4, treble: 0.3, pulse: kick, hue: t * 0.02, bands, ..Default::default() };
            px = e.render(&p).to_vec();
        }
        let mut out = format!("P6 {w} {h} 255\n").into_bytes();
        for c in px.chunks(4) { out.extend_from_slice(&c[..3]); }
        std::fs::write(std::env::temp_dir().join(format!("vs-{s}.ppm")), out).unwrap();
    }
}
