# BigWave Radio plugin guide (API v1)

This guide is written so that a person **or an AI assistant** can write a
working plugin from it alone. Everything a plugin can do is listed here.

## One-shot prompt (for AI assistants)

> You are writing a BigWave Radio plugin. Output exactly two files,
> `plugin.toml` and `main.rhai`, for a folder named after the plugin `id`.
> Use only the API in this guide. The language is **Rhai**, not Rust or JS.
> There is no filesystem or network access. Declare every permission you
> use. Never flash large areas of the screen faster than `ctx.flash_cap`.
> When you can run commands, test with
> `BigWaveRadio.exe --check-plugin <folder>` (or `bwr-app --check-plugin`)
> and fix every `error:` line it prints.

## 1. Where plugins live

```
%LOCALAPPDATA%\BigWaveRadio\plugins\
    plugins-state.json      <- written by the app (enabled, permissions, settings)
    PLUGIN_GUIDE.md         <- this file
    strobe\                 <- bundled Beat Strobe
        plugin.toml
        main.rhai
    my-plugin\
        plugin.toml
        main.rhai
        store.json          <- written by the app if you use storage
```

* **Install**: drop the folder in, or use **Plugins -> Import .zip** (or drag a
  `.zip` onto the window). The zip must contain `plugin.toml` at its root or
  inside one top-level folder. It is installed as `plugins\<id>` and replaces
  an older copy with the same id. A zip that fails the checks is refused.
* **Approve**: new plugins start as *needs approval*. The Plugins page lists
  the permissions they want; **Allow & enable** grants them. If a later
  version asks for more permissions it needs approval again.
* **Hot reload**: saving `plugin.toml` or any `.rhai` file reloads the plugin
  within a second (`on_unload`, then `on_load` with fresh state). Errors show
  on the plugin's card with line numbers, and in its Log.

## 2. plugin.toml

```toml
id = "my-plugin"            # required: 1-40 chars a-z 0-9 - _ ; also the folder name
name = "My Plugin"          # required
version = "1.0.0"           # required (free text)
api = 1                     # required: plugin API version (this host: 1)
runtime = "rhai"            # optional, only "rhai"
entry = "main.rhai"         # optional, default main.rhai
author = "You"              # optional
description = "One line"    # optional
permissions = ["audio", "overlay"]   # see the table below

[[pages]]                   # optional, needs the "page" permission
id = "main"                 # passed to on_page as page_id
title = "My Page"           # shown in the side bar
icon = "🎵"                 # optional single emoji

[[hotkeys]]                 # optional, needs the "hotkeys" permission
keys = "Ctrl+F,U"           # chord: hold Ctrl, press F, then U within ~1 s
action = "start"            # passed to on_hotkey(ctx, action)

[settings]                  # optional: the app builds a settings UI
speed = { type = "float",  default = 2.0, min = 0.5, max = 4.0, label = "Speed" }
count = { type = "int",    default = 3,   min = 1,   max = 10 }
on    = { type = "bool",   default = true }
tint  = { type = "color",  default = "#ff8800" }
name  = { type = "string", default = "hi" }
mode  = { type = "choice", default = "a", options = ["a", "b"] }
```

Settings are read with `ctx.settings.<key>`. They are read-only for the script.

### Permissions

| permission | allows |
|---|---|
| `audio` | `on_audio` (levels, bands, beat, BPM, breaks) |
| `now_playing` | `on_station_change`, `on_meta`, `on_player` |
| `player_control` | `ctx.player_stop/toggle/next/previous`, `ctx.set_volume` |
| `overlay` | `ctx.open_overlay(false)`, `on_overlay`, `on_overlay_closed` |
| `fullscreen` | `ctx.open_overlay(true)` (also needs `overlay`) |
| `hotkeys` | `[[hotkeys]]` + `on_hotkey` (app-wide shortcuts), and `on_key` while your overlay is open |
| `panel` | `on_panel` (a panel on your card on the Plugins page) |
| `page` | `[[pages]]` + `on_page` (whole pages in the side bar), `ctx.open_page` |
| `storage` | `ctx.store_get/store_set` (saved to `store.json`) |

There is **no** `net` or file permission in API v1. Asking for one is an error.

## 3. Hooks

All hooks are optional plain functions. Parameter counts must match.
A hook whose permission is missing is never called.

| hook | when | notes |
|---|---|---|
| `on_load(ctx)` | plugin starts (also after a hot reload) | set up `ctx.state` |
| `on_unload(ctx)` | plugin stops or reloads | |
| `on_station_change(ctx, station)` | a station starts | `station` = `#{name, url, tags, country, codec, bitrate}` |
| `on_meta(ctx, title)` | ICY song title changes | usually `"Artist - Song"` |
| `on_player(ctx, state)` | player state | `"connecting"`, `"playing"`, `"stopped"`, `"failed"` |
| `on_audio(ctx, a)` | every frame (~30-60 per second) | see audio fields |
| `on_tick(ctx, dt)` | every frame, after `on_audio` | `dt` = seconds since last frame |
| `on_overlay(ctx, draw)` | every frame while your overlay is open | draw with `draw` |
| `on_overlay_closed(ctx)` | your overlay was closed (any reason) | reset state |
| `on_key(ctx, key)` | key press / click while your overlay is open | return `true` if handled |
| `on_hotkey(ctx, action)` | one of your `[[hotkeys]]` was pressed anywhere in the app | `action` from plugin.toml |
| `on_panel(ctx, ui)` | each frame your card is visible on the Plugins page | widgets with `ui` |
| `on_page(ctx, page_id, ui)` | each frame one of your pages is open | widgets with `ui` |

### Audio fields (`a` in `on_audio`)

| field | type | range / meaning |
|---|---|---|
| `a.level` | float | RMS loudness 0..1 |
| `a.peak` | float | sample peak 0..1 |
| `a.bands` | array of 32 floats | spectrum 40 Hz..12 kHz, each 0..1 (auto-gained) |
| `a.bass`, `a.mid`, `a.treble` | float | band averages 0..1 |
| `a.beat` | bool | true on the frame a beat (bass onset) is detected |
| `a.bpm` | float | tempo estimate, 0 until known |
| `a.quiet` | bool | true during a break / silence / sharp drop in loudness |
| `a.playing` | bool | a station is playing |

The analysis is delayed to match what is coming out of the speakers.

### Hotkeys (`[[hotkeys]]`)

`keys` is one to three steps separated by commas; each step is
`Mod+Mod+Key` with `Ctrl`, `Shift`, `Alt` and an egui key name (`A`..`Z`,
`0`..`9`, `F1`.., `ArrowUp`, ...). The **first step must use Ctrl or Alt**
and `Esc` cannot be used. Later steps may be pressed with the first step's
modifiers still held (`Ctrl+F,U` fires for Ctrl+F then U, or Ctrl+F then
Ctrl+U). Each step must follow within about 1 second. Keys that are part of
a chord are swallowed, so they never reach the rest of the app. Hotkeys work
anywhere in the app window while no overlay is open. The app shows the
first hotkey next to your nav page in the side bar, e.g. `Strobe  Ctrl+F,U`.

If `on_hotkey` opens your overlay, it starts **without** the warning screen
(the user started it deliberately). The flash cap and Esc still apply.

### Keys (`on_key`)

Names: `"ArrowUp"`, `"ArrowDown"`, `"ArrowLeft"`, `"ArrowRight"`, `"Enter"`,
`"Space"`, `"Tab"`, `"Backspace"`, `"A"`..`"Z"`, `"0"`..`"9"`, `"F1"`.., plus
`"MouseClick"` for a click. **`Escape` never reaches the plugin**: the app
always closes the overlay itself. If `on_key` returns anything but `true`, or
errors, the app closes the overlay. Without the `hotkeys` permission every key
closes it.

## 4. `ctx`

| member | description |
|---|---|
| `ctx.state` | a map that keeps your data between calls (read/write) |
| `ctx.settings` | map of settings from plugin.toml (read-only) |
| `ctx.now` | seconds since the app started (float) |
| `ctx.flash_cap` | max flashes per second the user allows (3 by default, at most 12) |
| `ctx.overlay_active` | your overlay is open and past the warning screen |
| `ctx.is_fullscreen` | your overlay is fullscreen |
| `ctx.id` | your plugin id |
| `ctx.log(msg)` | append to your Log on the Plugins page |
| `ctx.toast(msg)` | small pop-up message |
| `ctx.open_overlay(fullscreen)` | open your overlay (`true` = fullscreen); the app shows a photosensitivity warning first |
| `ctx.close_overlay()` | close it |
| `ctx.fullscreen(on)` | same as `open_overlay(true)` / `close_overlay()` |
| `ctx.open_page(page_id)` | switch the app to one of your pages |
| `ctx.player_stop()`, `ctx.player_toggle()`, `ctx.player_next()`, `ctx.player_previous()` | playback |
| `ctx.set_volume(v)` | 0..1 |
| `ctx.store_get(key)` / `ctx.store_set(key, value)` | persistent storage (max 200 keys); missing key -> `()` |

## 5. Drawing (`draw` in `on_overlay`, or `ui.canvas(h)`)

Coordinates are fractions: `(0, 0)` top-left, `(1, 1)` bottom-right.
Radii and symbol sizes are fractions of the **height**. Colours are strings
`"#rgb"`, `"#rrggbb"` or `"#rrggbbaa"`; `rgb(r, g, b)` / `rgba(r, g, b, a)`
build them from 0..255 ints. Numbers may be ints or floats.

| call | draws |
|---|---|
| `draw.fill(color)` | the whole area |
| `draw.rect(x, y, w, h, color)` | filled rectangle |
| `draw.circle(x, y, r, color)` | filled circle |
| `draw.ring(x, y, r, width_px, color)` | circle outline |
| `draw.line(x1, y1, x2, y2, width_px, color)` | line |
| `draw.text(x, y, text, color)` / `draw.text(x, y, text, size_px, color)` | text, top-left at (x, y) |
| `draw.text_center(x, y, text, size_px, color)` | text centred on (x, y) |
| `draw.svg(path_d, cx, cy, size, width_px, color)` | SVG path line art |
| `draw.rings(cx, cy, radius, ring_width, [colors])` | a disc of concentric rings centred on (cx, cy); `radius` and `ring_width` are fractions of the height; colours cycle from the outside in; `"#00000000"` (transparent) leaves a gap. One command however many rings (max 400 per call) |

`draw.svg` takes SVG path data (the `d` attribute) in a **100x100 viewBox**,
centred at `(cx, cy)` and `size` tall (fraction of height). It is drawn as
strokes (outlines). Supported commands: `M L H V C S Q T Z` (upper and lower
case). Arcs (`A`) are not supported: use `C` curves or `draw.circle`.
A circle of radius r at (cx, cy) as cubic curves:
`M cx-r cy C cx-r cy-0.55r cx-0.55r cy-r cx cy-r C ...` (see the strobe's
`symbols()` for complete examples).

**Moiré / interference rings**: draw two `draw.rings` discs that move,
the first with `[ink, "#00000000"]` and the second with
`["#00000000", background]` on top. Where they overlap the ring gaps
interfere (the classic Amiga demo effect; see the bundled strobe's
`draw_rings`). Moving rings count as motion, not flashing (see section 9).

Max 5000 draw commands per frame.

## 6. Widgets (`ui` in `on_panel` / `on_page`)

Widgets are drawn top to bottom in call order. Results arrive on the next
frame, so call the same widgets in the same order every frame.

| call | returns |
|---|---|
| `ui.heading(text)`, `ui.label(text)`, `ui.small(text)` | - |
| `ui.separator()`, `ui.space(px)` | - |
| `ui.button(text)` | `true` on the frame after it was clicked |
| `ui.checkbox(text, value)` | the (possibly changed) bool |
| `ui.slider(text, value, min, max)` | the (possibly changed) float |
| `ui.choice(text, value, ["a", "b"])` | the (possibly changed) string |
| `ui.canvas(height_px)` | a `draw` object for a drawing area |

Typical pattern: `ctx.state.speed = ui.slider("Speed", ctx.state.speed, 1, 10);`

## 7. Helpers

`rand()` (0..1), `rand_int(lo, hi)` (inclusive), `rand_pick(array)`,
`clamp(x, lo, hi)`, `fmin(a, b)`, `fmax(a, b)` (accept ints or floats,
return floats), `rgb(...)`, `rgba(...)`, plus Rhai's built-ins
(`parse_float`, `to_string`, `len`, `push`, `split`, `round`, `abs`, ...).

## 8. Rhai gotchas (read this if you usually write Rust/JS/Python)

* No `struct`/`class`. Use maps: `#{ a: 1, b: "x" }` and arrays `[1, 2]`.
* `let` is required for new variables. Semicolons end statements; the last
  expression of a block (no `;`) is its value.
* **Functions cannot see variables outside themselves.** Keep data in
  `ctx.state`, and pass values as parameters. Top-level `let`/`const` are not
  visible in hooks. For constant tables write a function: `fn colours() { ["#f00", "#0f0"] }`.
* Reading many fields? Copy, change, write back:
  `let s = ctx.state; s.x += 1; s.y = 2; ctx.state = s;`
  (single assignments like `ctx.state.x = 1;` also work).
* Parameters are copies. To change a map in a helper, call it as a method:
  `fn bump(n) { this.count += n; }` then `s.bump(1);`.
* Ints and floats are different types: `1` vs `1.0`. Mixing them in
  arithmetic is fine (`x * 2`), but `ctx.state.speed = 3` stores an int.
  `fmin/fmax/clamp` always return floats.
* Strings: `"text"`, interpolation with backticks: `` `${name} at ${bpm} BPM` ``.
* `if` is an expression: `let c = if on { "#fff" } else { "#000" };`
* Loops: `for x in array { }`, `for i in 0..10 { }`, `while`, `loop` + `break`.
* A missing map key reads as `()` (unit). Check with `type_of(x) == "()"`.
* `print` output is discarded; use `ctx.log`.
* Each hook call has an operation and time budget (~25 ms). Infinite loops
  are stopped. A plugin that errors 3 times is stopped until it is reloaded.

## 9. Flashing and safety rules (mandatory)

* The app shows a photosensitivity warning before any overlay; the user must
  press Enter / click to continue. The only exception is an overlay opened
  from `on_hotkey`, because the user pressed the plugin's hotkey on purpose.
* The app estimates each overlay frame's average brightness. A rise of 10 %
  or more over the darkest recent level is a flash, and flashes are limited
  to `ctx.flash_cap` per second (3 by default; the user can opt in up to 12).
  Extra flashes have their large bright shapes painted black (or the whole
  frame, if needed), so design for the cap: use `1.0 / ctx.flash_cap` as the
  minimum gap between flashes. Ring discs count with their average colour,
  so swinging rings are motion, not flashing.
* Esc, any unhandled key and clicks close the overlay. Don't try to trap the user.
* Avoid saturated red full-screen flashes; prefer short white or dim colours.

## 10. Examples

All of these pass `--check-plugin` (they live in `docs/plugin-examples/` in the repo).
The bundled **Beat Strobe** (`plugins/strobe/`) is the full overlay example:
warning screen, beat sync, break detection, arrow-key speed, SVG symbols.

### Now playing log (panel + storage)

`plugin.toml`
```toml
id = "now-playing-log"
name = "Now Playing Log"
version = "1.0.0"
api = 1
description = "Keeps a list of the last 15 songs heard, with the station name."
permissions = ["now_playing", "panel", "storage"]
```
`main.rhai`
```rust
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
```

### Bass border (in-window overlay)

`plugin.toml`
```toml
id = "bass-border"
name = "Bass Border"
version = "1.0.0"
api = 1
description = "A glowing border that pulses with the bass, drawn in-window over the app."
permissions = ["audio", "overlay", "hotkeys", "panel"]

[settings]
color = { type = "color", default = "#3aa0ff", label = "Border colour" }
```
`main.rhai`
```rust
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
```

### Skip silent stations (player control)

`plugin.toml`
```toml
id = "silence-skip"
name = "Silence Skipper"
version = "1.0.0"
api = 1
description = "Skips to the next station in the list after 20 seconds of silence."
permissions = ["audio", "player_control", "now_playing", "panel"]

[settings]
seconds = { type = "int", default = 20, min = 5, max = 120, label = "Seconds of silence" }
```
`main.rhai`
```rust
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
```

### Visualiser page (adds a nav page)

`plugin.toml`
```toml
id = "mini-visualiser"
name = "Mini Visualiser"
version = "1.0.0"
api = 1
description = "Adds a whole Visualiser page to the side bar: spectrum bars and a BPM readout."
permissions = ["audio", "page"]

[[pages]]
id = "vis"
title = "Visualiser"
icon = "🎵"
```
`main.rhai`
```rust
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
```

## 11. Checklist before you hand over a plugin

1. `id` matches the folder name; `api = 1`.
2. Every API you call has its permission declared, and no extra ones.
3. Hooks have the exact parameter counts in section 3.
4. All values you draw with are clamped to sensible ranges.
5. Any flashing respects `ctx.flash_cap`, and `on_key` returns `true` only
   for keys you handle.
6. `--check-plugin <folder>` prints `OK` with no warnings.
