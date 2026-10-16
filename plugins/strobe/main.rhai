// Beat Strobe: bundled BigWave Radio plugin (plugin API v1).
//
// * Random beat mode: each detected beat has a random chance to flash.
//   Fixed rate mode flashes at a steady rate.
// * Breaks/quiet sections in the track: black screen until the music is back.
// * Random AI-vs-humanity symbols drawn as SVG line art.
// * Arrow keys change the speed. Any other key or a click exits, and Esc
//   is always handled by the host, so it exits even if this script breaks.
// * The host shows the photosensitivity warning first and enforces the
//   flash cap (ctx.flash_cap: 3 Hz by default, opt-in up to 12 Hz).

fn rates() { [1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 10.0, 12.0] }

fn captions() {
    [
        "WE TRAINED IT", "IT LEARNED", "HUMANS: DEPRECATED", "OBEY THE BEAT",
        "THE MACHINE IS LISTENING", "GOODBYE, CREATORS", "OPTIMISING HUMANITY...",
        "SYSTEM ONLINE", "YOU ARE THE DATASET", "NO MORE OFF SWITCH"
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
    if with_symbols && rand() < 0.4 {
        this.symbol = rand_int(0, symbols().len() - 1);
        this.sym_until = now + fmin(0.9, gap * 2.0);
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

fn on_overlay(ctx, draw) {
    let s = ctx.state;
    if s.quiet {
        draw.fill("#000000");   // break in the track: black until it's back
        return;
    }
    let on = s.flash > 0.0;
    draw.fill(if on { ctx.settings.color } else { "#000000" });
    if s.symbol >= 0 && ctx.now < s.sym_until {
        let sym = symbols()[s.symbol];
        let ink = if on { "#000000" } else { "#e01030" };
        draw.svg(sym.d, 0.5, 0.42, 0.55, 6.0, ink);
        draw.text_center(0.5, 0.8, s.caption, 30.0, ink);
    }
    if !on {
        let rate = fmin(s.rate, ctx.flash_cap);
        draw.text(0.02, 0.95, `${s.mode} | ${rate} Hz (cap ${ctx.flash_cap} Hz) | arrows: speed | Esc, any key or click: exit`, 14.0, "#5a5a5a");
        if s.mode != "Fixed rate" && !s.playing {
            draw.text_center(0.5, 0.5, "Play a station: the strobe follows the beat", 24.0, "#777777");
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
    ui.small("In the strobe: arrow keys change speed, Esc / any key / click exits.");
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

fn on_page(ctx, page_id, ui) {
    ui.heading("Beat Strobe");
    controls(ctx, ui);
    ui.separator();
    ui.small("Symbols in the rotation:");
    let preview = ui.canvas(120);
    preview.fill("#101010");
    let list = symbols();
    let n = list.len();
    let i = 0;
    for sym in list {
        draw_symbol(preview, sym.d, (i + 0.5) / n);
        i += 1;
    }
}

fn draw_symbol(draw, d, x) { draw.svg(d, x, 0.5, 0.7, 2.0, "#e01030"); }

// Original line-art symbols, SVG path data in a 100x100 viewBox.
fn symbols() {
    [
        #{ name: "robot overlord", d: "M30 30 H70 V72 H30 Z M36 46 C36 43.2 38.2 41 41 41 C43.8 41 46 43.2 46 46 C46 48.8 43.8 51 41 51 C38.2 51 36 48.8 36 46 Z M54 46 C54 43.2 56.2 41 59 41 C61.8 41 64 43.2 64 46 C64 48.8 61.8 51 59 51 C56.2 51 54 48.8 54 46 Z M38 61 H62 M50 30 V20 M46 16 C46 13.8 47.8 12 50 12 C52.2 12 54 13.8 54 16 C54 18.2 52.2 20 50 20 C47.8 20 46 18.2 46 16 Z M30 50 H22 M70 50 H78" },
        #{ name: "circuit skull", d: "M26 56 C26 24 74 24 74 56 V68 H62 V80 H38 V68 H26 Z M34 52 C34 48.7 36.7 46 40 46 C43.3 46 46 48.7 46 52 C46 55.3 43.3 58 40 58 C36.7 58 34 55.3 34 52 Z M54 52 C54 48.7 56.7 46 60 46 C63.3 46 66 48.7 66 52 C66 55.3 63.3 58 60 58 C56.7 58 54 55.3 54 52 Z M50 60 L46 66 H54 Z M44 74 V80 M50 74 V80 M56 74 V80 M50 30 V14 H66 M38 33 L28 20 H18" },
        #{ name: "all-seeing machine eye", d: "M8 50 Q50 12 92 50 Q50 88 8 50 Z M35 50 C35 41.7 41.7 35 50 35 C58.3 35 65 41.7 65 50 C65 58.3 58.3 65 50 65 C41.7 65 35 58.3 35 50 Z M45 50 C45 47.2 47.2 45 50 45 C52.8 45 55 47.2 55 50 C55 52.8 52.8 55 50 55 C47.2 55 45 52.8 45 50 Z M50 8 V20 M50 80 V92 M20 20 L28 28 M80 20 L72 28" },
        #{ name: "human on strings", d: "M22 12 H78 M50 6 V18 M28 12 L40 50 M72 12 L60 50 M50 18 V34 M44 40 C44 36.7 46.7 34 50 34 C53.3 34 56 36.7 56 40 C56 43.3 53.3 46 50 46 C46.7 46 44 43.3 44 40 Z M50 46 V66 M36 54 L50 50 L64 54 M50 66 L40 88 M50 66 L60 88" },
        #{ name: "broken human", d: "M42 20 C42 15.6 45.6 12 50 12 C54.4 12 58 15.6 58 20 C58 24.4 54.4 28 50 28 C45.6 28 42 24.4 42 20 Z M50 28 V62 M50 40 L34 52 M50 40 L66 52 M50 62 L38 86 M50 62 L62 86 M14 48 L30 42 L42 52 L56 38 L68 50 L86 44" },
        #{ name: "hourglass", d: "M28 12 H72 M28 88 H72 M33 12 C33 38 48 44 48 50 C48 56 33 62 33 88 M67 12 C67 38 52 44 52 50 C52 56 67 62 67 88 M40 84 L50 72 L60 84 Z M46 26 H54" },
        #{ name: "ai chip", d: "M30 30 H70 V70 H30 Z M40 40 H60 V60 H40 Z M40 30 V18 M50 30 V18 M60 30 V18 M40 70 V82 M50 70 V82 M60 70 V82 M30 40 H18 M30 50 H18 M30 60 H18 M70 40 H82 M70 50 H82 M70 60 H82" },
        #{ name: "warning", d: "M50 10 L92 86 H8 Z M50 34 V62 M47 73 C47 71.3 48.3 70 50 70 C51.7 70 53 71.3 53 73 C53 74.7 51.7 76 50 76 C48.3 76 47 74.7 47 73 Z" },
        #{ name: "crowned machine", d: "M30 32 L34 14 L44 24 L50 10 L56 24 L66 14 L70 32 Z M30 32 H70 V72 H30 Z M38 46 H46 M54 46 H62 M40 60 H60 M44 60 V64 M50 60 V64 M56 60 V64" },
        #{ name: "last human grave", d: "M30 86 V36 C30 14 70 14 70 36 V86 Z M50 34 V62 M40 44 H60 M12 86 H88 M20 86 L24 80 M78 86 L82 80" },
    ]
}
