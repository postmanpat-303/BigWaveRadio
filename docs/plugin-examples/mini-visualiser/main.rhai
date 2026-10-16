fn on_load(ctx) { ctx.state.bands = []; ctx.state.bpm = 0.0; ctx.state.beat_glow = 0.0; }

fn on_audio(ctx, a) {
    let s = ctx.state;
    s.bands = a.bands;
    s.bpm = a.bpm;
    if a.beat { s.beat_glow = 1.0; }
    ctx.state = s;
}

fn on_tick(ctx, dt) { ctx.state.beat_glow = fmax(0.0, ctx.state.beat_glow - dt * 3.0); }

fn on_page(ctx, page_id, ui) {
    ui.heading("Visualiser");
    let bpm = if ctx.state.bpm > 0.0 { `${ctx.state.bpm.round()} BPM` } else { "BPM: listening..." };
    ui.label(bpm);
    let d = ui.canvas(260);
    d.fill("#0b0b12");
    let n = ctx.state.bands.len();
    if n == 0 { d.text_center(0.5, 0.5, "Play a station", 18, "#666666"); return; }
    let w = 1.0 / n;
    let i = 0;
    for b in ctx.state.bands {
        d.rect(i * w + w * 0.1, 1.0 - b, w * 0.8, b, rgb(40 + (i * 6), 120, 255));
        i += 1;
    }
    // Small beat dot (not a full-screen flash).
    d.circle(0.95, 0.1, 0.04 * ctx.state.beat_glow + 0.005, "#ff3060");
}
