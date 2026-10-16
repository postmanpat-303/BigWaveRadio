fn on_load(ctx) {
    ctx.state.silent_for = 0.0;
    ctx.state.playing = false;
    ctx.state.skips = 0;
}

fn on_player(ctx, state) {
    ctx.state.playing = state == "playing";
    ctx.state.silent_for = 0.0;
}

fn on_audio(ctx, a) {
    if !ctx.state.playing { return; }
    let s = ctx.state;
    s.silent_for = if a.level < 0.005 { s.silent_for + 1.0 / 60.0 } else { 0.0 };
    if s.silent_for >= ctx.settings.seconds {
        s.silent_for = 0.0;
        s.skips += 1;
        ctx.log("silence: skipping to the next station");
        ctx.toast("Silent stream, skipping");
        ctx.player_next();
    }
    ctx.state = s;
}

fn on_panel(ctx, ui) {
    ui.label(`Skipped ${ctx.state.skips} silent station(s) this session.`);
}
