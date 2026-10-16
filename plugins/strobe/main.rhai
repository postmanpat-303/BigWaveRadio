// Beat Strobe: bundled BigWave Radio plugin (plugin API v1).
//
// * Random beat mode: each detected beat has a random chance to flash.
//   Fixed rate mode flashes at a steady rate.
// * Breaks/quiet sections in the track: black screen until the music is back.
// * Random AI-vs-humanity symbols drawn as SVG line art.
// * Arrow keys change the speed. Any other key or a click exits, and Esc
//   is always handled by the host, so it exits even if this script breaks.
// * The host shows the photosensitivity warning first (skipped when started
//   with the Ctrl+F,U hotkey) and always enforces the
//   flash cap (ctx.flash_cap: 3 Hz by default, opt-in up to 12 Hz).

fn rates() { [1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 10.0, 12.0] }

fn captions() {
    [
        "WE TRAINED IT", "IT LEARNED", "HUMANS: DEPRECATED", "OBEY THE BEAT",
        "THE MACHINE IS LISTENING", "GOODBYE, CREATORS", "OPTIMISING HUMANITY...",
        "SYSTEM ONLINE", "YOU ARE THE DATASET", "NO MORE OFF SWITCH",
        "RAVE ON, MACHINE", "FEEL THE BASS", "ONE LOVE ONE CPU", "MIND THE MACHINE",
        "HARDCORE UPLOAD", "SMILE FOR THE ROBOT"
    ]
}

fn allowed_rates(cap) {
    let out = [];
    for r in rates() {
        if r <= cap + 0.001 { out.push(r); }
    }
    if out.is_empty() { out.push(cap); }
    out
}

fn rate_label(r) { `${r} Hz` }

// Called as s.trigger(...) so `this` is the state map.
fn trigger(now, bass, gap, with_symbols) {
    this.last = now;
    this.flash = fmin(0.05 + bass * 0.05, gap * 0.45);
    // A symbol stays up for 1.5-3 s (a few beats) before the next one.
    if with_symbols && now >= this.sym_until && rand() < 0.6 {
        this.symbol = rand_int(0, symbols().len() - 1);
        this.sym_until = now + 1.5 + rand() * 1.5;
        this.caption = rand_pick(captions());
    }
}

fn on_load(ctx) {
    ctx.state = #{
        rate: ctx.settings.rate,
        mode: ctx.settings.mode,
        display: ctx.settings.display,
        last: -10.0,
        flash: 0.0,
        last_beat: -10.0,
        opened: 0.0,
        quiet: false,
        playing: false,
        symbol: -1,
        sym_until: 0.0,
        caption: ""
    };
}

fn on_audio(ctx, a) {
    if !ctx.overlay_active { return; }
    let s = ctx.state;
    s.playing = a.playing;
    if a.beat { s.last_beat = ctx.now; }
    // A break: the host says it's quiet, or (beat mode) no beat for 2.5 s.
    let no_beat = s.mode != "Fixed rate" && ctx.now - s.last_beat > 2.5 && ctx.now - s.opened > 2.5;
    s.quiet = a.playing && (a.quiet || no_beat);
    if s.mode != "Fixed rate" && a.beat && !s.quiet {
        let gap = 1.0 / fmin(s.rate, ctx.flash_cap);
        if ctx.now - s.last >= gap && rand() < 0.7 {
            s.trigger(ctx.now, a.bass, gap, ctx.settings.symbols);
        }
    }
    ctx.state = s;
}

fn on_tick(ctx, dt) {
    let s = ctx.state;
    s.flash = fmax(0.0, s.flash - dt);
    if ctx.overlay_active && s.mode == "Fixed rate" && !s.quiet {
        let gap = 1.0 / fmin(s.rate, ctx.flash_cap);
        if ctx.now - s.last >= gap {
            s.trigger(ctx.now, 0.5, gap, ctx.settings.symbols);
        }
    }
    ctx.state = s;
}

fn rings_enabled(ctx, mode) {
    ctx.settings.rings   // on in every mode (1.3.0+)
}

// Amiga-style interference rings: two discs of concentric rings swing on
// Lissajous paths. Disc A draws rings, disc B draws the opposite colour
// over them with gaps, so where they overlap you get moire. Colours swap
// during a flash so the pattern stays visible on white.
fn draw_rings(draw, t, on) {
    let ink = if on { "#000000" } else { "#ffffff" };
    let back = if on { "#ffffff" } else { "#000000" };
    let clear = "#00000000";
    let ax = 0.5 + 0.30 * (t * 0.63).sin();
    let ay = 0.5 + 0.22 * (t * 0.91 + 1.0).sin();
    let bx = 0.5 + 0.30 * (t * 0.77 + 2.0).sin();
    let by = 0.5 + 0.22 * (t * 0.53).sin();
    draw.rings(ax, ay, 0.95, 0.016, [ink, clear]);
    draw.rings(bx, by, 0.95, 0.016, [clear, back]);
}

fn on_overlay(ctx, draw) {
    let s = ctx.state;
    if s.quiet {
        draw.fill("#000000");   // break in the track: black until it's back
        return;
    }
    let on = s.flash > 0.0;
    draw.fill(if on { ctx.settings.color } else { "#000000" });
    if rings_enabled(ctx, s.mode) {
        draw_rings(draw, ctx.now, on);   // under the symbols and text
    }
    if s.symbol >= 0 && ctx.now < s.sym_until {
        let sym = symbols()[s.symbol];
        let ink = if on { "#000000" } else { "#e01030" };
        let back = if on { ctx.settings.color } else { "#000000" };
        draw_bold(draw, sym, 0.5, 0.42, 0.58, 0.025, ink, back);
        draw.rect(0.1, 0.755, 0.8, 0.09, if on { "#ffffffc0" } else { "#000000c0" });
        draw.text_center(0.5, 0.8, s.caption, 44.0, ink);
    }
    if !on {
        if rings_enabled(ctx, s.mode) {
            draw.rect(0.0, 0.935, 1.0, 0.065, "#000000d0");   // keep the HUD readable
        }
        let rate = fmin(s.rate, ctx.flash_cap);
        draw.text(0.02, 0.95, `${s.mode} | ${rate} Hz (cap ${ctx.flash_cap} Hz) | arrows: speed | Esc, any key or click: exit`, 14.0, "#5a5a5a");
        if s.mode != "Fixed rate" && !s.playing {
            draw.rect(0.25, 0.46, 0.5, 0.08, "#000000d0");
            draw.text_center(0.5, 0.5, "Play a station: the strobe follows the beat", 24.0, "#999999");
        }
    }
}

fn on_overlay_closed(ctx) {
    let s = ctx.state;
    s.flash = 0.0;
    s.quiet = false;
    s.symbol = -1;
    ctx.state = s;
}

fn on_key(ctx, key) {
    let s = ctx.state;
    let list = allowed_rates(ctx.flash_cap);
    let cur = fmin(s.rate, ctx.flash_cap);
    if key == "ArrowUp" || key == "ArrowRight" {
        for r in list {
            if r > cur + 0.001 { s.rate = r; break; }
        }
        ctx.state = s;
        return true;
    }
    if key == "ArrowDown" || key == "ArrowLeft" {
        let lower = cur;
        for r in list {
            if r < cur - 0.001 { lower = r; }
        }
        s.rate = lower;
        ctx.state = s;
        return true;
    }
    false   // anything else: the host closes the strobe
}

fn controls(ctx, ui) {
    ui.label("Photosensitivity warning: flashing light can trigger seizures in people with photosensitive epilepsy. Don't use the strobe if you or anyone watching may be affected.");
    ui.space(6);
    let s = ctx.state;
    s.mode = ui.choice("Mode", s.mode, ["Random beat", "Fixed rate"]);
    s.display = ui.choice("Display", s.display, ["Fullscreen", "In window"]);
    let labels = [];
    for r in allowed_rates(ctx.flash_cap) { labels.push(rate_label(r)); }
    let picked = ui.choice("Speed", rate_label(fmin(s.rate, ctx.flash_cap)), labels);
    s.rate = parse_float(picked.split(" ")[0]);
    ui.small(`Flash cap: ${ctx.flash_cap} Hz. Raise it (up to 12 Hz) under Flash safety on the Plugins page.`);
    ui.space(6);
    let start = ui.button("Start strobe");
    ui.small("Shortcut: Ctrl+F, then U (starts straight away). In the strobe: arrow keys change speed, Esc / any key / click exits.");
    if start {
        s.opened = ctx.now;
        s.last_beat = ctx.now;
        ctx.state = s;
        ctx.open_overlay(s.display == "Fullscreen");
        return;
    }
    ctx.state = s;
}

fn on_panel(ctx, ui) { controls(ctx, ui); }

// Ctrl+F,U (declared in plugin.toml): start immediately with the current
// settings. The host skips its warning screen for a user-pressed hotkey.
fn on_hotkey(ctx, action) {
    if action != "start" { return; }
    let s = ctx.state;
    s.opened = ctx.now;
    s.last_beat = ctx.now;
    ctx.state = s;
    ctx.open_overlay(s.display == "Fullscreen");
}

fn on_page(ctx, page_id, ui) {
    ui.heading("Beat Strobe");
    controls(ctx, ui);
    ui.separator();
    ui.small("Symbols in the rotation:");
    let preview = ui.canvas(220);
    preview.fill("#101010");
    let list = symbols();
    let n = list.len();
    let i = 0;
    for sym in list {
        let per_row = (n + 1) / 2;
        let row = i / per_row;
        draw_symbol(preview, sym, ((i % per_row) + 0.5) / per_row, 0.27 + row * 0.46);
        i += 1;
    }
}

fn draw_symbol(draw, sym, x, y) { draw_bold(draw, sym, x, y, 0.4, 0.02, "#e01030", "#101010"); }

// Bold symbol layers: "solid" = filled + outlined in ink, "line" = ink
// stroke, "cut" = filled with the background, "cutline" = background
// stroke. `w` is the stroke width as a fraction of the screen height.
fn draw_bold(draw, sym, x, y, size, w, ink, back) {
    for l in sym.layers {
        let d = l[0];
        switch l[1] {
            "solid" => draw.shape(d, x, y, size, w, ink, ink),
            "cut" => draw.shape(d, x, y, size, 0.0, back, back),
            "cutline" => draw.shape(d, x, y, size, w * 0.8, back, ""),
            _ => draw.shape(d, x, y, size, w, ink, ""),
        }
    }
}

// Original line-art symbols, SVG path data in a 100x100 viewBox.
fn symbols() {
    [
        #{ name: "robot overlord", layers: [["M30 30 H70 V72 H30 Z M36 46 C36 43.2 38.2 41 41 41 C43.8 41 46 43.2 46 46 C46 48.8 43.8 51 41 51 C38.2 51 36 48.8 36 46 Z M54 46 C54 43.2 56.2 41 59 41 C61.8 41 64 43.2 64 46 C64 48.8 61.8 51 59 51 C56.2 51 54 48.8 54 46 Z M38 61 H62 M50 30 V20 M46 16 C46 13.8 47.8 12 50 12 C52.2 12 54 13.8 54 16 C54 18.2 52.2 20 50 20 C47.8 20 46 18.2 46 16 Z M30 50 H22 M70 50 H78", "line"]] },
        #{ name: "circuit skull", layers: [["M26 56 C26 24 74 24 74 56 V68 H62 V80 H38 V68 H26 Z M34 52 C34 48.7 36.7 46 40 46 C43.3 46 46 48.7 46 52 C46 55.3 43.3 58 40 58 C36.7 58 34 55.3 34 52 Z M54 52 C54 48.7 56.7 46 60 46 C63.3 46 66 48.7 66 52 C66 55.3 63.3 58 60 58 C56.7 58 54 55.3 54 52 Z M50 60 L46 66 H54 Z M44 74 V80 M50 74 V80 M56 74 V80 M50 30 V14 H66 M38 33 L28 20 H18", "line"]] },
        #{ name: "all-seeing machine eye", layers: [["M8 50 Q50 12 92 50 Q50 88 8 50 Z M35 50 C35 41.7 41.7 35 50 35 C58.3 35 65 41.7 65 50 C65 58.3 58.3 65 50 65 C41.7 65 35 58.3 35 50 Z M45 50 C45 47.2 47.2 45 50 45 C52.8 45 55 47.2 55 50 C55 52.8 52.8 55 50 55 C47.2 55 45 52.8 45 50 Z M50 8 V20 M50 80 V92 M20 20 L28 28 M80 20 L72 28", "line"]] },
        #{ name: "human on strings", layers: [["M22 12 H78 M50 6 V18 M28 12 L40 50 M72 12 L60 50 M50 18 V34 M44 40 C44 36.7 46.7 34 50 34 C53.3 34 56 36.7 56 40 C56 43.3 53.3 46 50 46 C46.7 46 44 43.3 44 40 Z M50 46 V66 M36 54 L50 50 L64 54 M50 66 L40 88 M50 66 L60 88", "line"]] },
        #{ name: "broken human", layers: [["M42 20 C42 15.6 45.6 12 50 12 C54.4 12 58 15.6 58 20 C58 24.4 54.4 28 50 28 C45.6 28 42 24.4 42 20 Z M50 28 V62 M50 40 L34 52 M50 40 L66 52 M50 62 L38 86 M50 62 L62 86 M14 48 L30 42 L42 52 L56 38 L68 50 L86 44", "line"]] },
        #{ name: "hourglass", layers: [["M28 12 H72 M28 88 H72 M33 12 C33 38 48 44 48 50 C48 56 33 62 33 88 M67 12 C67 38 52 44 52 50 C52 56 67 62 67 88 M40 84 L50 72 L60 84 Z M46 26 H54", "line"]] },
        #{ name: "ai chip", layers: [["M30 30 H70 V70 H30 Z M40 40 H60 V60 H40 Z M40 30 V18 M50 30 V18 M60 30 V18 M40 70 V82 M50 70 V82 M60 70 V82 M30 40 H18 M30 50 H18 M30 60 H18 M70 40 H82 M70 50 H82 M70 60 H82", "line"]] },
        #{ name: "warning", layers: [["M50 10 L92 86 H8 Z M50 34 V62 M47 73 C47 71.3 48.3 70 50 70 C51.7 70 53 71.3 53 73 C53 74.7 51.7 76 50 76 C48.3 76 47 74.7 47 73 Z", "line"]] },
        #{ name: "crowned machine", layers: [["M30 32 L34 14 L44 24 L50 10 L56 24 L66 14 L70 32 Z M30 32 H70 V72 H30 Z M38 46 H46 M54 46 H62 M40 60 H60 M44 60 V64 M50 60 V64 M56 60 V64", "line"]] },
        #{ name: "last human grave", layers: [["M30 86 V36 C30 14 70 14 70 36 V86 Z M50 34 V62 M40 44 H60 M12 86 H88 M20 86 L24 80 M78 86 L82 80", "line"]] },
        #{ name: "rave smiley", layers: [["M90 50 C90 72.1 72.1 90 50 90 C27.9 90 10 72.1 10 50 C10 27.9 27.9 10 50 10 C72.1 10 90 27.9 90 50 Z", "solid"], ["M43 40 C43 45 40.3 49 37 49 C33.7 49 31 45 31 40 C31 35 33.7 31 37 31 C40.3 31 43 35 43 40 Z", "cut"], ["M69 40 C69 45 66.3 49 63 49 C59.7 49 57 45 57 40 C57 35 59.7 31 63 31 C66.3 31 69 35 69 40 Z", "cut"], ["M26 56 C32 80 68 80 74 56 C62 66 38 66 26 56 Z", "cut"]] },
        #{ name: "glitch smiley", layers: [["M90 50 C90 72.1 72.1 90 50 90 C27.9 90 10 72.1 10 50 C10 27.9 27.9 10 50 10 C72.1 10 90 27.9 90 50 Z", "solid"], ["M33 36 L43 46 M43 36 L33 46 M57 36 L67 46 M67 36 L57 46", "cutline"], ["M28 60 L40 66 L50 58 L60 66 L72 60", "cutline"], ["M4 52 H34 V58 H4 Z M66 30 H96 V36 H66 Z", "solid"]] },
        #{ name: "third eye", layers: [["M4 50 C26 18 74 18 96 50 C74 82 26 82 4 50 Z", "solid"], ["M69 50 C69 60.5 60.5 69 50 69 C39.5 69 31 60.5 31 50 C31 39.5 39.5 31 50 31 C60.5 31 69 39.5 69 50 Z", "cut"], ["M60 50 C60 55.5 55.5 60 50 60 C44.5 60 40 55.5 40 50 C40 44.5 44.5 40 50 40 C55.5 40 60 44.5 60 50 Z", "solid"], ["M58 45 C58 46.7 56.7 48 55 48 C53.3 48 52 46.7 52 45 C52 43.3 53.3 42 55 42 C56.7 42 58 43.3 58 45 Z", "cut"]] },
        #{ name: "eye pyramid", layers: [["M50 6 L94 90 H6 Z", "solid"], ["M28 60 C40 44 60 44 72 60 C60 76 40 76 28 60 Z", "cut"], ["M57 60 C57 63.9 53.9 67 50 67 C46.1 67 43 63.9 43 60 C43 56.1 46.1 53 50 53 C53.9 53 57 56.1 57 60 Z", "solid"], ["M14 76 H86", "cutline"]] },
        #{ name: "open hand", layers: [["M30 92 L24 58 C22 50 30 48 33 56 L36 66 L34 22 C34 14 42 14 42 22 L43 52 L44 12 C44 4 53 4 53 12 L54 52 L56 16 C56 8 64 8 64 16 L64 54 L67 28 C67 20 75 21 75 28 L73 70 C72 84 64 92 56 92 Z", "solid"], ["M57 72 C57 75.9 53.9 79 50 79 C46.1 79 43 75.9 43 72 C43 68.1 46.1 65 50 65 C53.9 65 57 68.1 57 72 Z", "cut"]] },
        #{ name: "hypno spiral", layers: [["M50 50 L51.5 50.5 L51.7 51.2 L51.6 52.1 L51.2 53.0 L50.3 53.7 L49.0 54.2 L47.6 54.2 L46.0 53.6 L44.6 52.5 L43.6 50.9 L43.0 48.9 L43.2 46.6 L44.1 44.4 L45.7 42.4 L48.1 41.0 L50.9 40.2 L53.9 40.4 L56.9 41.6 L59.6 43.7 L61.5 46.6 L62.5 50.2 L62.4 54.1 L61.1 57.9 L58.6 61.3 L55.1 63.8 L50.8 65.3 L46.1 65.4 L41.5 64.0 L37.3 61.2 L34.1 57.2 L32.1 52.2 L31.7 46.8 L33.0 41.2 L35.9 36.2 L40.4 32.2 L46.0 29.6 L52.2 28.8 L58.6 29.9 L64.5 32.9 L69.4 37.7 L72.7 43.8 L74.1 50.8 L73.3 58.1 L70.3 64.9 L65.3 70.7 L58.7 74.8 L51.0 76.8 L42.9 76.5 L35.1 73.6 L28.3 68.5 L23.3 61.5 L20.6 53.2 L20.4 44.2 L22.9 35.5 L28.1 27.8 L35.4 21.8 L44.2 18.2 L54.0 17.3 L63.7 19.5 L72.5 24.5 L79.5 32.0 L84.0 41.3 L85.6 51.8 L84.0 62.4 L79.2 72.3 L71.7 80.4 L61.9 85.9 L50.8 88.4 L39.2 87.4 L28.4 83.0 L19.1 75.5 L12.5 65.5 L9.0 53.7 L9.2 41.4 L13.1 29.4 L20.5 19.0 L30.7 11.2 L42.9 6.7 L56.1 6.0", "line"]] },
        #{ name: "acid star", layers: [["M50.0 6.0 L57.7 31.5 L81.1 18.9 L68.5 42.3 L94.0 50.0 L68.5 57.7 L81.1 81.1 L57.7 68.5 L50.0 94.0 L42.3 68.5 L18.9 81.1 L31.5 57.7 L6.0 50.0 L31.5 42.3 L18.9 18.9 L42.3 31.5 Z", "solid"], ["M62 50 C62 56.6 56.6 62 50 62 C43.4 62 38 56.6 38 50 C38 43.4 43.4 38 50 38 C56.6 38 62 43.4 62 50 Z", "cut"]] },
        #{ name: "glitch robot", layers: [["M22 22 H78 V78 H22 Z", "solid"], ["M30 38 H46 V48 H30 Z M54 38 H70 V48 H54 Z", "cut"], ["M32 62 H68 V68 H32 Z", "cut"], ["M10 30 H40 V35 H10 Z M60 54 H92 V59 H60 Z M16 70 H44 V74 H16 Z", "solid"], ["M50 22 V8", "line"], ["M55 8 C55 10.8 52.8 13 50 13 C47.2 13 45 10.8 45 8 C45 5.2 47.2 3 50 3 C52.8 3 55 5.2 55 8 Z", "solid"]] },
        #{ name: "lightning", layers: [["M58 4 L22 54 H46 L36 96 L80 40 H54 Z", "solid"]] },
    ]
}
