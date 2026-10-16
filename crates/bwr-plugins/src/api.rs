//! The Rhai engine and the script-facing API (`ctx`, `draw`, `ui`).

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rhai::{Array, Dynamic, Engine, EvalAltResult, Map};

use crate::draw::{parse_svg_path, DrawCmd, Rgba};
use crate::manifest::Permission;
use crate::ui::{text_of, Ui, UiResult, Widget};

type Res<T> = Result<T, Box<EvalAltResult>>;

/// Something a plugin asked the host to do; drained after each hook call.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    OpenOverlay { fullscreen: bool },
    CloseOverlay,
    Toast(String),
    PlayerStop,
    PlayerToggle,
    PlayerNext,
    PlayerPrevious,
    SetVolume(f32),
    OpenPage(String),
}

pub(crate) struct CtxData {
    pub plugin_id: String,
    pub perms: BTreeSet<Permission>,
    pub state: Map,
    pub settings: Map,
    pub store: Map,
    pub store_dirty: bool,
    pub now: f64,
    pub flash_cap: f64,
    pub overlay_active: bool,
    pub fullscreen: bool,
    pub requests: Vec<Request>,
    pub logs: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct Ctx(pub Rc<RefCell<CtxData>>);

impl Ctx {
    pub fn new(plugin_id: &str, perms: BTreeSet<Permission>, settings: Map, store: Map) -> Self {
        Ctx(Rc::new(RefCell::new(CtxData {
            plugin_id: plugin_id.to_string(),
            perms,
            state: Map::new(),
            settings,
            store,
            store_dirty: false,
            now: 0.0,
            flash_cap: crate::DEFAULT_FLASH_HZ,
            overlay_active: false,
            fullscreen: false,
            requests: Vec::new(),
            logs: Vec::new(),
        })))
    }

    fn need(&self, p: Permission, what: &str) -> Res<()> {
        if self.0.borrow().perms.contains(&p) {
            Ok(())
        } else {
            Err(format!("{what} needs the \"{}\" permission in plugin.toml", p.name()).into())
        }
    }

    fn request(&self, r: Request) {
        self.0.borrow_mut().requests.push(r);
    }
}

#[derive(Clone, Default)]
pub(crate) struct Draw(pub Rc<RefCell<Vec<DrawCmd>>>);

impl Draw {
    fn push(&self, c: DrawCmd) -> Res<()> {
        let mut v = self.0.borrow_mut();
        if v.len() >= 5000 {
            return Err("too many draw commands in one frame (max 5000)".into());
        }
        v.push(c);
        Ok(())
    }
}

fn num(d: &Dynamic, what: &str) -> Res<f32> {
    if let Ok(f) = d.as_float() {
        return Ok(f as f32);
    }
    if let Ok(i) = d.as_int() {
        return Ok(i as f32);
    }
    Err(format!("{what}: expected a number, got {}", d.type_name()).into())
}

fn color(d: &Dynamic) -> Res<Rgba> {
    let s = text_of(d);
    Rgba::parse(&s).ok_or_else(|| format!("bad colour \"{s}\" (use \"#rrggbb\" or \"#rrggbbaa\")").into())
}

thread_local! {
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
    static RNG: Cell<u64> = Cell::new(
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1
    );
}

/// Run `f` with a wall-clock budget for script execution.
pub(crate) fn with_deadline<T>(budget: Duration, f: impl FnOnce() -> T) -> T {
    DEADLINE.with(|d| d.set(Some(Instant::now() + budget)));
    let out = f();
    DEADLINE.with(|d| d.set(None));
    out
}

fn rand_f64() -> f64 {
    RNG.with(|r| {
        let mut x = r.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        r.set(x);
        (x >> 11) as f64 / (1u64 << 53) as f64
    })
}

pub(crate) fn build_engine() -> Engine {
    let mut e = Engine::new();
    e.set_max_operations(500_000);
    e.set_max_call_levels(48);
    e.set_max_expr_depths(64, 32);
    e.set_max_string_size(64 * 1024);
    e.set_max_array_size(20_000);
    e.set_max_map_size(5_000);
    e.disable_symbol("eval");
    e.on_progress(|ops| {
        if ops % 1024 == 0 {
            let late = DEADLINE.with(|d| d.get().is_some_and(|t| Instant::now() > t));
            if late {
                return Some("plugin took too long (time limit exceeded)".into());
            }
        }
        None
    });
    e.on_print(|_| {}); // print() output is routed through ctx.log instead

    // ---- helpers -------------------------------------------------------------
    e.register_fn("rand", rand_f64);
    e.register_fn("rand_int", |lo: i64, hi: i64| -> i64 {
        if hi <= lo {
            lo
        } else {
            lo + (rand_f64() * (hi - lo + 1) as f64) as i64
        }
    });
    e.register_fn("rand_pick", |a: Array| -> Dynamic {
        if a.is_empty() {
            Dynamic::UNIT
        } else {
            a[(rand_f64() * a.len() as f64) as usize % a.len()].clone()
        }
    });
    e.register_fn("clamp", |x: Dynamic, lo: Dynamic, hi: Dynamic| -> Res<f64> {
        Ok((num(&x, "clamp")? as f64).clamp(num(&lo, "clamp")? as f64, num(&hi, "clamp")? as f64))
    });
    e.register_fn("fmax", |a: Dynamic, b: Dynamic| -> Res<f64> { Ok((num(&a, "fmax")?).max(num(&b, "fmax")?) as f64) });
    e.register_fn("fmin", |a: Dynamic, b: Dynamic| -> Res<f64> { Ok((num(&a, "fmin")?).min(num(&b, "fmin")?) as f64) });
    e.register_fn("rgb", |r: i64, g: i64, b: i64| format!("#{:02x}{:02x}{:02x}", r.clamp(0, 255), g.clamp(0, 255), b.clamp(0, 255)));
    e.register_fn("rgba", |r: i64, g: i64, b: i64, a: i64| {
        format!("#{:02x}{:02x}{:02x}{:02x}", r.clamp(0, 255), g.clamp(0, 255), b.clamp(0, 255), a.clamp(0, 255))
    });

    // ---- ctx -------------------------------------------------------------------
    e.register_type_with_name::<Ctx>("Ctx");
    e.register_get_set("state", |c: &mut Ctx| c.0.borrow().state.clone(), |c: &mut Ctx, m: Map| c.0.borrow_mut().state = m);
    e.register_get("settings", |c: &mut Ctx| c.0.borrow().settings.clone());
    e.register_get("now", |c: &mut Ctx| c.0.borrow().now);
    e.register_get("flash_cap", |c: &mut Ctx| c.0.borrow().flash_cap);
    e.register_get("overlay_active", |c: &mut Ctx| c.0.borrow().overlay_active);
    e.register_get("is_fullscreen", |c: &mut Ctx| c.0.borrow().fullscreen);
    e.register_get("id", |c: &mut Ctx| c.0.borrow().plugin_id.clone());
    e.register_fn("log", |c: &mut Ctx, msg: Dynamic| {
        let mut d = c.0.borrow_mut();
        if d.logs.len() < 200 {
            d.logs.push(text_of(&msg));
        }
    });
    e.register_fn("toast", |c: &mut Ctx, msg: Dynamic| c.request(Request::Toast(text_of(&msg))));
    e.register_fn("open_overlay", |c: &mut Ctx, fullscreen: bool| -> Res<()> {
        c.need(Permission::Overlay, "ctx.open_overlay")?;
        if fullscreen {
            c.need(Permission::Fullscreen, "ctx.open_overlay(true)")?;
        }
        c.request(Request::OpenOverlay { fullscreen });
        Ok(())
    });
    e.register_fn("close_overlay", |c: &mut Ctx| c.request(Request::CloseOverlay));
    e.register_fn("fullscreen", |c: &mut Ctx, on: bool| -> Res<()> {
        if on {
            c.need(Permission::Overlay, "ctx.fullscreen")?;
            c.need(Permission::Fullscreen, "ctx.fullscreen")?;
            c.request(Request::OpenOverlay { fullscreen: true });
        } else {
            c.request(Request::CloseOverlay);
        }
        Ok(())
    });
    e.register_fn("open_page", |c: &mut Ctx, page: Dynamic| -> Res<()> {
        c.need(Permission::Page, "ctx.open_page")?;
        c.request(Request::OpenPage(text_of(&page)));
        Ok(())
    });
    for (name, req) in [
        ("player_stop", Request::PlayerStop),
        ("player_toggle", Request::PlayerToggle),
        ("player_next", Request::PlayerNext),
        ("player_previous", Request::PlayerPrevious),
    ] {
        e.register_fn(name, move |c: &mut Ctx| -> Res<()> {
            c.need(Permission::PlayerControl, &format!("ctx.{name}"))?;
            c.request(req.clone());
            Ok(())
        });
    }
    e.register_fn("set_volume", |c: &mut Ctx, v: Dynamic| -> Res<()> {
        c.need(Permission::PlayerControl, "ctx.set_volume")?;
        c.request(Request::SetVolume(num(&v, "set_volume")?.clamp(0.0, 1.0)));
        Ok(())
    });
    e.register_fn("store_get", |c: &mut Ctx, key: &str| -> Res<Dynamic> {
        c.need(Permission::Storage, "ctx.store_get")?;
        Ok(c.0.borrow().store.get(key).cloned().unwrap_or(Dynamic::UNIT))
    });
    e.register_fn("store_set", |c: &mut Ctx, key: &str, v: Dynamic| -> Res<()> {
        c.need(Permission::Storage, "ctx.store_set")?;
        let mut d = c.0.borrow_mut();
        if d.store.len() >= 200 && !d.store.contains_key(key) {
            return Err("storage is limited to 200 keys".into());
        }
        d.store.insert(key.into(), v);
        d.store_dirty = true;
        Ok(())
    });

    // ---- draw ------------------------------------------------------------------
    e.register_type_with_name::<Draw>("Draw");
    e.register_fn("fill", |d: &mut Draw, c: Dynamic| -> Res<()> { d.push(DrawCmd::Fill(color(&c)?)) });
    e.register_fn("rect", |d: &mut Draw, x: Dynamic, y: Dynamic, w: Dynamic, h: Dynamic, c: Dynamic| -> Res<()> {
        d.push(DrawCmd::Rect { x: num(&x, "rect")?, y: num(&y, "rect")?, w: num(&w, "rect")?, h: num(&h, "rect")?, color: color(&c)? })
    });
    e.register_fn("circle", |d: &mut Draw, x: Dynamic, y: Dynamic, r: Dynamic, c: Dynamic| -> Res<()> {
        d.push(DrawCmd::Circle { x: num(&x, "circle")?, y: num(&y, "circle")?, r: num(&r, "circle")?, color: color(&c)?, stroke: None })
    });
    e.register_fn("ring", |d: &mut Draw, x: Dynamic, y: Dynamic, r: Dynamic, w: Dynamic, c: Dynamic| -> Res<()> {
        d.push(DrawCmd::Circle {
            x: num(&x, "ring")?,
            y: num(&y, "ring")?,
            r: num(&r, "ring")?,
            color: color(&c)?,
            stroke: Some(num(&w, "ring")?.clamp(0.5, 50.0)),
        })
    });
    e.register_fn("line", |d: &mut Draw, x1: Dynamic, y1: Dynamic, x2: Dynamic, y2: Dynamic, w: Dynamic, c: Dynamic| -> Res<()> {
        d.push(DrawCmd::Line {
            x1: num(&x1, "line")?,
            y1: num(&y1, "line")?,
            x2: num(&x2, "line")?,
            y2: num(&y2, "line")?,
            width: num(&w, "line")?.clamp(0.5, 50.0),
            color: color(&c)?,
        })
    });
    fn text(d: &mut Draw, x: Dynamic, y: Dynamic, t: Dynamic, size: f32, c: Dynamic, centered: bool) -> Res<()> {
        d.push(DrawCmd::Text { x: num(&x, "text")?, y: num(&y, "text")?, text: text_of(&t), size: size.clamp(6.0, 400.0), color: color(&c)?, centered })
    }
    e.register_fn("text", |d: &mut Draw, x: Dynamic, y: Dynamic, t: Dynamic, c: Dynamic| text(d, x, y, t, 16.0, c, false));
    e.register_fn("text", |d: &mut Draw, x: Dynamic, y: Dynamic, t: Dynamic, s: Dynamic, c: Dynamic| -> Res<()> {
        text(d, x, y, t, num(&s, "text")?, c, false)
    });
    e.register_fn("text_center", |d: &mut Draw, x: Dynamic, y: Dynamic, t: Dynamic, s: Dynamic, c: Dynamic| -> Res<()> {
        text(d, x, y, t, num(&s, "text_center")?, c, true)
    });
    // draw.svg(path_d, cx, cy, size, stroke_px, colour): path in a 100x100
    // viewBox, centred on (cx, cy), `size` = fraction of the area height.
    e.register_fn(
        "svg",
        |d: &mut Draw, path: Dynamic, cx: Dynamic, cy: Dynamic, size: Dynamic, w: Dynamic, c: Dynamic| -> Res<()> {
            let polys = parse_svg_path(&text_of(&path)).map_err(|e| format!("draw.svg: {e}"))?;
            let (cx, cy, size) = (num(&cx, "svg")?, num(&cy, "svg")?, num(&size, "svg")?);
            let paths = polys
                .into_iter()
                .map(|p| p.into_iter().map(|(x, y)| ((x - 50.0) / 100.0 * size, (y - 50.0) / 100.0 * size)).collect())
                .collect();
            d.push(DrawCmd::Paths { cx, cy, paths, width: num(&w, "svg")?.clamp(0.5, 50.0), color: color(&c)? })
        },
    );

    // draw.rings(cx, cy, radius, ring_width, [colours]): concentric rings,
    // one host command however many rings there are.
    e.register_fn(
        "rings",
        |d: &mut Draw, cx: Dynamic, cy: Dynamic, r: Dynamic, w: Dynamic, cols: Array| -> Res<()> {
            let (radius, width) = (num(&r, "rings")?.clamp(0.0, 10.0), num(&w, "rings")?);
            if width <= 0.0 {
                return Err("draw.rings: ring_width must be > 0".into());
            }
            if (radius / width) as usize > crate::draw::MAX_RINGS {
                return Err(format!("draw.rings: too many rings (radius / ring_width > {})", crate::draw::MAX_RINGS).into());
            }
            if cols.is_empty() {
                return Err("draw.rings: needs at least one colour".into());
            }
            let colors = cols.iter().map(color).collect::<Res<Vec<_>>>()?;
            d.push(DrawCmd::Rings { cx: num(&cx, "rings")?, cy: num(&cy, "rings")?, radius, width, colors })
        },
    );

    // ---- ui ----------------------------------------------------------------------
    e.register_type_with_name::<Ui>("Ui");
    e.register_fn("heading", |u: &mut Ui, t: Dynamic| {
        u.push(Widget::Heading(text_of(&t)));
    });
    e.register_fn("label", |u: &mut Ui, t: Dynamic| {
        u.push(Widget::Label(text_of(&t)));
    });
    e.register_fn("small", |u: &mut Ui, t: Dynamic| {
        u.push(Widget::Small(text_of(&t)));
    });
    e.register_fn("separator", |u: &mut Ui| {
        u.push(Widget::Separator);
    });
    e.register_fn("space", |u: &mut Ui, px: Dynamic| -> Res<()> {
        u.push(Widget::Space(num(&px, "space")?.clamp(0.0, 400.0)));
        Ok(())
    });
    e.register_fn("button", |u: &mut Ui, t: Dynamic| -> bool {
        let i = u.push(Widget::Button(text_of(&t)));
        matches!(u.result(i), Some(UiResult::Clicked))
    });
    e.register_fn("checkbox", |u: &mut Ui, t: Dynamic, v: bool| -> bool {
        let i = u.push(Widget::Checkbox { label: text_of(&t), value: v });
        match u.result(i) {
            Some(UiResult::Bool(b)) => b,
            _ => v,
        }
    });
    e.register_fn("slider", |u: &mut Ui, t: Dynamic, v: Dynamic, lo: Dynamic, hi: Dynamic| -> Res<f64> {
        let (v, lo, hi) = (num(&v, "slider")? as f64, num(&lo, "slider")? as f64, num(&hi, "slider")? as f64);
        let i = u.push(Widget::Slider { label: text_of(&t), value: v, min: lo, max: hi });
        Ok(match u.result(i) {
            Some(UiResult::Float(f)) => f.clamp(lo.min(hi), hi.max(lo)),
            _ => v,
        })
    });
    e.register_fn("choice", |u: &mut Ui, t: Dynamic, v: Dynamic, opts: Array| -> String {
        let options: Vec<String> = opts.iter().map(text_of).collect();
        let v = text_of(&v);
        let i = u.push(Widget::Choice { label: text_of(&t), value: v.clone(), options: options.clone() });
        match u.result(i) {
            Some(UiResult::Text(s)) if options.contains(&s) => s,
            _ => v,
        }
    });
    e.register_fn("canvas", |u: &mut Ui, h: Dynamic| -> Res<Draw> {
        let i = u.push(Widget::Canvas { height: num(&h, "canvas")?.clamp(10.0, 2000.0), cmds: Vec::new() });
        let d = Draw::default();
        u.0.borrow_mut().canvases.push((i, d.clone()));
        Ok(d)
    });
    e
}

