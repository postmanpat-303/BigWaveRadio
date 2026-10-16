// No flashing: the border only changes thickness, never the whole screen.
fn on_load(ctx) { ctx.state.bass = 0.0; }

fn on_audio(ctx, a) {
    // Smooth so it pulses instead of flickering.
    ctx.state.bass = ctx.state.bass * 0.8 + a.bass * 0.2;
}

fn on_panel(ctx, ui) {
    if ui.button("Show bass border (any key exits)") { ctx.open_overlay(false); }
}

fn on_overlay(ctx, draw) {
    let t = 0.01 + ctx.state.bass * 0.05;     // thickness as a fraction
    let c = ctx.settings.color;
    draw.rect(0.0, 0.0, 1.0, t, c);
    draw.rect(0.0, 1.0 - t, 1.0, t, c);
    draw.rect(0.0, 0.0, t * 0.6, 1.0, c);
    draw.rect(1.0 - t * 0.6, 0.0, t * 0.6, 1.0, c);
}

fn on_key(ctx, key) { false }   // not consumed -> host closes the overlay
