fn on_load(ctx) {
    let saved = ctx.store_get("history");
    ctx.state.history = if type_of(saved) == "array" { saved } else { [] };
    ctx.state.station = "";
}

fn on_station_change(ctx, station) {
    ctx.state.station = station.name;
}

fn on_meta(ctx, title) {
    if title == "" { return; }
    let s = ctx.state;
    s.history.insert(0, `${title}  (${s.station})`);
    if s.history.len() > 15 { s.history.truncate(15); }
    ctx.state = s;
    ctx.store_set("history", s.history);
}

fn on_panel(ctx, ui) {
    if ctx.state.history.is_empty() {
        ui.small("No songs yet. Play a station that sends titles.");
        return;
    }
    for line in ctx.state.history { ui.label(line); }
    if ui.button("Clear") {
        ctx.state.history = [];
        ctx.store_set("history", []);
    }
}
