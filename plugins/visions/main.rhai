// Visions: bundled BigWave Radio visualiser (plugin API v1).
//
// All the heavy lifting is done by the host's native visual engine
// (draw.visual): this script only chooses scenes, palettes, motion and how
// audio drives them. The engine is non-strobing: its brightness can only
// change slowly, and beats become gentle pulses and colour shifts.
//
// Keys in the visualiser: Left / Right switch scene, Esc (or any other key,
// or a click) exits.

// Scene presets. `scene` is a draw.visual scene; the rest are its
// feedback and colour parameters.
fn presets() {
    [
        #{ name: "Feedback Tunnel", scene: "tunnel", zoom: 1.035, rotate: 0.004, warp: 0.3, decay: 0.90, detail: 1.0, speed: 1.0,
           palette: ["#0b0420", "#3a0ca3", "#f72585", "#ffb703", "#4cc9f0"] },
        #{ name: "Kaleidoscope", scene: "kaleido", zoom: 1.015, rotate: 0.012, warp: 0.5, decay: 0.93, detail: 1.2, speed: 0.8,
           palette: ["#ff006e", "#fb5607", "#ffbe0b", "#3a86ff", "#8338ec", "#06d6a0"] },
        #{ name: "Plasma", scene: "plasma", zoom: 1.0, rotate: 0.0, warp: 0.2, decay: 0.86, detail: 1.0, speed: 0.9,
           palette: ["#240046", "#7b2cbf", "#e0aaff", "#00f5d4", "#fee440", "#f15bb5"] },
        #{ name: "Flow Field", scene: "flow", zoom: 1.002, rotate: 0.0, warp: 0.1, decay: 0.965, detail: 1.0, speed: 1.0,
           palette: ["#003300", "#00ff66", "#ccff00", "#00cc88", "#66ffcc"] },
        #{ name: "Wire Sphere", scene: "sphere", zoom: 1.012, rotate: -0.003, warp: 0.0, decay: 0.86, detail: 1.0, speed: 1.0,
           palette: ["#00f0ff", "#0077ff", "#ffffff", "#7b2ff7", "#00ffa3"] },
        #{ name: "Spectrum Terrain", scene: "terrain", zoom: 1.0, rotate: 0.0, warp: 0.0, decay: 0.55, detail: 1.0, speed: 1.0,
           palette: ["#2b00ff", "#00e5ff", "#00ff85", "#ffe600", "#ff2e63"] },
        #{ name: "Starfield", scene: "stars", zoom: 1.0, rotate: 0.0, warp: 0.0, decay: 0.78, detail: 1.0, speed: 1.0,
           palette: ["#ffffff", "#9bd1ff", "#ffd6f6", "#b5fffc"] },
        #{ name: "Mandala", scene: "mandala", zoom: 1.02, rotate: 0.006, warp: 0.25, decay: 0.92, detail: 3.0, speed: 1.0,
           palette: ["#ff00c8", "#ffe600", "#00ffd5", "#7a00ff", "#ff6b00"] },
    ]
}

fn secs(label) {
    if label == "Off" { return 0.0; }
    parse_float(label.split(" ")[0])
}

fn quality_of(label) {
    if label == "Low" { 0.6 } else if label == "High" { 1.4 } else { 1.0 }
}

fn lerp(a, b, t) { a + (b - a) * t }

fn on_load(ctx) {
    ctx.state = #{
        cur: 0, next: -1, fade_from: 0.0, fade_len: 2.5, changed: 0.0,
        level: 0.0, bass: 0.0, mid: 0.0, treble: 0.0, pulse: 0.0, hue: 0.0,
        bands: [], last_beat_change: 0.0, opened: 0.0, label_until: 0.0
    };
}

// Start a crossfade to scene `to`.
fn go_to(s, now, to, len) {
    let n = presets().len();
    if s.next >= 0 { s.cur = s.next; }
    s.next = (to + n) % n;
    s.fade_from = now;
    s.fade_len = len;
    s.changed = now;
    s.label_until = now + 2.5;
    s
}

fn on_audio(ctx, a) {
    let s = ctx.state;
    // Smooth everything: visuals glide instead of jumping.
    s.level = s.level * 0.85 + a.level * 0.15;
    s.bass = s.bass * 0.8 + a.bass * 0.2;
    s.mid = s.mid * 0.85 + a.mid * 0.15;
    s.treble = s.treble * 0.85 + a.treble * 0.15;
    let b = [];
    let old = s.bands;
    let i = 0;
    for v in a.bands {
        let prev = if i < old.len() { old[i] } else { 0.0 };
        b.push(prev * 0.7 + v * 0.3);
        i += 1;
    }
    s.bands = b;
    if a.beat {
        s.pulse = fmin(1.0, s.pulse + 0.6 * (0.5 + a.bass));
        s.hue += 0.015 + a.bass * 0.02;          // colour shift on the beat
        let strong = a.bass > 0.7;
        if ctx.overlay_active && ctx.settings.beat_change && strong && s.next < 0 && ctx.now - s.changed > 6.0 {
            s = go_to(s, ctx.now, s.cur + 1, 1.5);
        }
    }
    ctx.state = s;
}

fn on_tick(ctx, dt) {
    let s = ctx.state;
    s.pulse = fmax(0.0, s.pulse - dt * 2.5);
    s.hue += dt * 0.01;
    if s.next >= 0 && ctx.now - s.fade_from >= s.fade_len {
        s.cur = s.next;
        s.next = -1;
    }
    let every = secs(ctx.settings.cycle);
    if ctx.overlay_active && every > 0.0 && s.next < 0 && ctx.now - s.changed >= every {
        s = go_to(s, ctx.now, s.cur + 1, 2.5);
    }
    ctx.state = s;
}

// Parameters for draw.visual: the current scene, crossfading into the next.
fn params(ctx, s, id, quality) {
    let list = presets();
    let a = list[s.cur];
    let mix = 0.0;
    let b = a;
    if s.next >= 0 {
        b = list[s.next];
        let x = fmin(1.0, (ctx.now - s.fade_from) / s.fade_len);
        mix = x * x * (3.0 - 2.0 * x);
    }
    #{
        id: id,
        scene: a.scene,
        scene_b: if s.next >= 0 { b.scene } else { "" },
        mix: mix,
        time: ctx.now,
        palette: if mix < 0.5 { a.palette } else { b.palette },
        zoom: lerp(a.zoom, b.zoom, mix),
        rotate: lerp(a.rotate, b.rotate, mix),
        warp: lerp(a.warp, b.warp, mix),
        decay: lerp(a.decay, b.decay, mix),
        detail: lerp(a.detail, b.detail, mix),
        speed: lerp(a.speed, b.speed, mix),
        hue: s.hue,
        intensity: ctx.settings.intensity,
        quality: quality,
        level: s.level, bass: s.bass, mid: s.mid, treble: s.treble,
        pulse: s.pulse,
        bands: s.bands
    }
}

fn on_overlay(ctx, draw) {
    let s = ctx.state;
    draw.fill("#000000");
    draw.visual(params(ctx, s, "visions", quality_of(ctx.settings.quality)));
    if ctx.settings.names && ctx.now < s.label_until {
        let name = presets()[if s.next >= 0 { s.next } else { s.cur }].name;
        let k = fmin(1.0, (s.label_until - ctx.now) / 0.8);
        let a = to_int(k * 200.0);
        let h = "0123456789abcdef";
        let hex = `${h.sub_string(a / 16, 1)}${h.sub_string(a % 16, 1)}`;
        draw.text(0.03, 0.05, name, 26.0, `#ffffff${hex}`);
    }
    if ctx.now - s.opened < 4.0 {
        draw.text(0.03, 0.95, "Left / Right: scene  |  Esc, any other key or click: exit", 14.0, "#ffffff90");
    }
}

fn on_key(ctx, key) {
    let s = ctx.state;
    if key == "ArrowRight" { ctx.state = go_to(s, ctx.now, (if s.next >= 0 { s.next } else { s.cur }) + 1, 1.0); return true; }
    if key == "ArrowLeft" { ctx.state = go_to(s, ctx.now, (if s.next >= 0 { s.next } else { s.cur }) - 1, 1.0); return true; }
    false   // anything else: the host closes the visualiser
}

fn start(ctx) {
    let s = ctx.state;
    s.opened = ctx.now;
    s.changed = ctx.now;
    s.label_until = ctx.now + 2.5;
    ctx.state = s;
    ctx.open_overlay(ctx.settings.display == "Fullscreen");
}

fn on_hotkey(ctx, action) {
    if action == "start" { start(ctx); }
}

fn on_panel(ctx, ui) {
    if ui.button("Start Visions") { start(ctx); }
    ui.small("Shortcut: Ctrl+M, then D. Left / Right switch scene, Esc exits.");
}

fn on_page(ctx, page_id, ui) {
    ui.heading("Visions");
    ui.label("A MilkDrop-style visualiser that reacts to the music. Smooth and non-strobing: beats become pulses and colour shifts.");
    ui.space(6);
    if ui.button("Start Visions") { start(ctx); }
    ui.small("Shortcut: Ctrl+M, then D. Left / Right switch scene, Esc exits. Settings (display, scene timing, quality) are on the Plugins page.");
    ui.space(6);
    // Live previews of every scene.
    let c = ui.canvas(300);
    c.fill("#06060c");
    let list = presets();
    let s = ctx.state;
    let i = 0;
    for p in list {
        let col = i % 4;
        let row = i / 4;
        let x = 0.01 + col * 0.2475;
        let y = 0.02 + row * 0.49;
        let q = #{ cur: i, next: -1, hue: s.hue, level: s.level, bass: s.bass, mid: s.mid, treble: s.treble, pulse: s.pulse, bands: s.bands };
        let v = params(ctx, q, `preview-${i}`, 0.35);
        v.rect = [x, y, 0.235, 0.40];
        v.intensity = 0.6 * ctx.settings.intensity;
        c.visual(v);
        c.text(x + 0.008, y + 0.415, p.name, 13.0, "#d0d0ff");
        i += 1;
    }
}
