//! Plugin discovery, lifecycle, hot reload, permissions and hook dispatch.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rhai::{CallFnOptions, Dynamic, Engine, Map, Scope, AST};
use serde::{Deserialize, Serialize};

use crate::api::{build_engine, with_deadline, Ctx, Draw, Request};
use crate::draw::DrawCmd;
use crate::manifest::{Manifest, PageSpec, Permission, SettingKind};
use crate::safety::FlashLimiter;
use crate::ui::{Ui, UiResult, Widget};
use crate::{DEFAULT_FLASH_HZ, HARD_MAX_FLASH_HZ};

/// Per-call script time budget (on top of the operation limit).
const HOOK_BUDGET: Duration = Duration::from_millis(25);
/// A plugin that errors this many times is stopped until it is reloaded.
const MAX_ERRORS: u32 = 3;
const STATE_FILE: &str = "plugins-state.json";

/// Known hooks and their parameter counts.
pub(crate) const HOOKS: &[(&str, usize, Option<Permission>)] = &[
    ("on_load", 1, None),
    ("on_unload", 1, None),
    ("on_station_change", 2, Some(Permission::NowPlaying)),
    ("on_meta", 2, Some(Permission::NowPlaying)),
    ("on_player", 2, Some(Permission::NowPlaying)),
    ("on_audio", 2, Some(Permission::Audio)),
    ("on_tick", 2, None),
    ("on_overlay", 2, Some(Permission::Overlay)),
    ("on_overlay_closed", 1, Some(Permission::Overlay)),
    ("on_panel", 2, Some(Permission::Panel)),
    ("on_page", 3, Some(Permission::Page)),
    ("on_key", 2, Some(Permission::Hotkeys)),
    ("on_hotkey", 2, Some(Permission::Hotkeys)),
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StationInfo {
    pub name: String,
    pub url: String,
    pub tags: String,
    pub country: String,
    pub codec: String,
    pub bitrate: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct AudioFrame {
    pub features: bwr_spectrum::AudioFeatures,
    pub bands: [f32; bwr_spectrum::BANDS],
    pub playing: bool,
}

impl Default for AudioFrame {
    fn default() -> Self {
        Self { features: Default::default(), bands: [0.0; bwr_spectrum::BANDS], playing: false }
    }
}

/// Persisted host decisions (which plugins are enabled, granted permissions,
/// settings overrides, flash safety). Lives in `plugins/plugins-state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HostState {
    pub enabled: BTreeMap<String, bool>,
    pub granted: BTreeMap<String, Vec<String>>,
    pub settings: BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
    /// User-chosen flash cap in Hz.
    pub flash_cap_hz: f64,
    /// Opt-in allowing the cap above 3 Hz (never above 12 Hz).
    pub allow_fast_flash: bool,
}

impl Default for HostState {
    fn default() -> Self {
        Self {
            enabled: BTreeMap::new(),
            granted: BTreeMap::new(),
            settings: BTreeMap::new(),
            flash_cap_hz: DEFAULT_FLASH_HZ,
            allow_fast_flash: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PluginStatus {
    Running,
    /// Turned off by the user.
    Disabled,
    /// Never enabled, or asks for permissions that were not granted yet.
    NeedsApproval,
    Error(String),
}

pub struct Plugin {
    pub dir: PathBuf,
    pub manifest: Option<Manifest>,
    pub status: PluginStatus,
    pub log: VecDeque<String>,
    ast: Option<AST>,
    ctx: Option<Ctx>,
    hooks: Vec<(String, usize)>,
    errors: u32,
    fingerprint: u64,
    ui_results: HashMap<String, HashMap<usize, UiResult>>,
}

impl Plugin {
    pub fn id(&self) -> String {
        self.manifest
            .as_ref()
            .map(|m| m.id.clone())
            .unwrap_or_else(|| self.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
    }

    pub fn name(&self) -> String {
        self.manifest.as_ref().map(|m| m.name.clone()).unwrap_or_else(|| self.id())
    }

    pub fn hook_list(&self) -> Vec<(String, usize)> {
        self.hooks.clone()
    }

    pub fn has_hook(&self, name: &str) -> bool {
        self.hooks.iter().any(|(n, _)| n == name)
    }

    pub fn has_perm(&self, p: Permission) -> bool {
        self.manifest.as_ref().is_some_and(|m| m.permission_set().contains(&p))
    }

    fn push_log(&mut self, line: String) {
        if self.log.len() >= 100 {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayPhase {
    /// Host-drawn photosensitivity warning; nothing from the plugin is shown.
    Warning,
    Running,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlaySession {
    pub plugin_id: String,
    pub fullscreen: bool,
    pub phase: OverlayPhase,
}

pub struct PluginHost {
    pub dir: PathBuf,
    pub plugins: Vec<Plugin>,
    pub state: HostState,
    pub overlay: Option<OverlaySession>,
    pub limiter: FlashLimiter,
    engine: Engine,
    requests: Vec<(String, Request)>,
    clock: Instant,
    last_poll: Instant,
    last_store_flush: Instant,
    /// Never write to disk (used by `--check-plugin`).
    readonly: bool,
    /// Set while `on_hotkey` runs: an overlay the plugin opens in response
    /// to the user's own key chord starts without the warning screen.
    hotkey_plugin: Option<String>,
    chords: crate::ChordMatcher,
}

fn fingerprint(dir: &Path) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if name == "plugin.toml" || name.ends_with(".rhai") {
                name.hash(&mut h);
                if let Ok(md) = e.metadata() {
                    md.len().hash(&mut h);
                    md.modified().ok().hash(&mut h);
                }
            }
        }
    }
    h.finish()
}

fn toml_to_dynamic(v: &toml::Value) -> Dynamic {
    match v {
        toml::Value::String(s) => s.clone().into(),
        toml::Value::Integer(i) => (*i).into(),
        toml::Value::Float(f) => (*f).into(),
        toml::Value::Boolean(b) => (*b).into(),
        other => other.to_string().into(),
    }
}

fn json_to_dynamic(v: &serde_json::Value) -> Dynamic {
    rhai::serde::to_dynamic(v).unwrap_or(Dynamic::UNIT)
}

/// One-off fixes to saved settings. Strobe 1.2 had `rings_fixed` (default
/// off), so rings never showed in the default Fixed rate mode; 1.3 uses one
/// `rings` switch for every mode. A saved `rings_fixed` is dropped, and a
/// saved `rings = false` from those versions is dropped too unless the user
/// also had Random beat mode selected (where it was a deliberate choice).
pub fn migrate_state(state: &mut HostState) -> bool {
    let Some(s) = state.settings.get_mut("strobe") else { return false };
    if s.remove("rings_fixed").is_none() {
        return false;
    }
    let random = s.get("mode").and_then(|m| m.as_str()) == Some("Random beat");
    if !random && s.get("rings") == Some(&serde_json::Value::Bool(false)) {
        s.remove("rings");
    }
    true
}

impl PluginHost {
    /// Create a host for `dir` (created if missing) and load every plugin.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let _ = std::fs::create_dir_all(&dir);
        let state = std::fs::read_to_string(dir.join(STATE_FILE))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let mut state: HostState = state;
        let migrated = migrate_state(&mut state);
        let mut host = Self {
            dir,
            plugins: Vec::new(),
            state,
            overlay: None,
            limiter: FlashLimiter::new(DEFAULT_FLASH_HZ),
            engine: build_engine(),
            requests: Vec::new(),
            clock: Instant::now(),
            last_poll: Instant::now(),
            last_store_flush: Instant::now(),
            readonly: false,
            hotkey_plugin: None,
            chords: Default::default(),
        };
        host.limiter.set_cap(host.flash_cap());
        if migrated {
            host.save_state();
        }
        host.scan();
        host
    }

    pub fn now(&self) -> f64 {
        self.clock.elapsed().as_secs_f64()
    }

    /// The effective host-enforced flash cap in Hz.
    pub fn flash_cap(&self) -> f64 {
        let max = if self.state.allow_fast_flash { HARD_MAX_FLASH_HZ } else { DEFAULT_FLASH_HZ };
        self.state.flash_cap_hz.clamp(0.5, max)
    }

    pub fn set_flash_safety(&mut self, allow_fast: bool, cap_hz: f64) {
        self.state.allow_fast_flash = allow_fast;
        self.state.flash_cap_hz = cap_hz.clamp(0.5, HARD_MAX_FLASH_HZ);
        self.state.flash_cap_hz = self.flash_cap();
        self.limiter.set_cap(self.flash_cap());
        self.save_state();
    }

    /// A throw-away host holding just one plugin folder with all of its
    /// permissions granted. Writes nothing to disk.
    pub fn detached(plugin_dir: &Path) -> Self {
        let mut host = Self {
            dir: plugin_dir.parent().map(Path::to_path_buf).unwrap_or_default(),
            plugins: Vec::new(),
            state: HostState::default(),
            overlay: None,
            limiter: FlashLimiter::new(DEFAULT_FLASH_HZ),
            engine: build_engine(),
            requests: Vec::new(),
            clock: Instant::now(),
            last_poll: Instant::now(),
            last_store_flush: Instant::now(),
            readonly: true,
            hotkey_plugin: None,
            chords: Default::default(),
        };
        let p = host.load_dir(plugin_dir);
        if let Some(m) = &p.manifest {
            host.state.enabled.insert(m.id.clone(), true);
            host.state.granted.insert(m.id.clone(), m.permissions.clone());
        }
        host.plugins.push(p);
        host.start_all_pending();
        host
    }

    pub fn save_state(&self) {
        if self.readonly {
            return;
        }
        if let Ok(s) = serde_json::to_string_pretty(&self.state) {
            let _ = std::fs::write(self.dir.join(STATE_FILE), s);
        }
    }

    fn plugin_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir() && p.join("plugin.toml").is_file())
                    .filter(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
                    .collect()
            })
            .unwrap_or_default();
        dirs.sort();
        dirs
    }

    /// Unload everything and load all plugin folders again.
    pub fn scan(&mut self) {
        for i in (0..self.plugins.len()).rev() {
            self.unload_at(i);
        }
        self.plugins.clear();
        for d in self.plugin_dirs() {
            let p = self.load_dir(&d);
            self.plugins.push(p);
        }
        self.start_all_pending();
    }

    /// Hot reload: call about once a frame; checks the disk once a second.
    pub fn poll_changes(&mut self) {
        if self.last_poll.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_poll = Instant::now();
        let dirs = self.plugin_dirs();
        // Removed plugins.
        for i in (0..self.plugins.len()).rev() {
            if !dirs.contains(&self.plugins[i].dir) {
                self.unload_at(i);
                self.plugins.remove(i);
            }
        }
        // New or changed plugins.
        for d in dirs {
            match self.plugins.iter().position(|p| p.dir == d) {
                Some(i) if self.plugins[i].fingerprint != fingerprint(&d) => self.reload_at(i),
                Some(_) => {}
                None => {
                    let p = self.load_dir(&d);
                    self.plugins.push(p);
                    let i = self.plugins.len() - 1;
                    self.start_at(i);
                }
            }
        }
        if self.last_store_flush.elapsed() > Duration::from_secs(5) {
            self.last_store_flush = Instant::now();
            for i in 0..self.plugins.len() {
                self.flush_store(i);
            }
        }
    }

    /// Parse + compile a plugin folder; does not run any code.
    fn load_dir(&self, dir: &Path) -> Plugin {
        let mut p = Plugin {
            dir: dir.to_path_buf(),
            manifest: None,
            status: PluginStatus::NeedsApproval,
            log: VecDeque::new(),
            ast: None,
            ctx: None,
            hooks: Vec::new(),
            errors: 0,
            fingerprint: fingerprint(dir),
            ui_results: HashMap::new(),
        };
        let src = match std::fs::read_to_string(dir.join("plugin.toml")) {
            Ok(s) => s,
            Err(e) => {
                p.status = PluginStatus::Error(format!("cannot read plugin.toml: {e}"));
                return p;
            }
        };
        let manifest = match Manifest::parse(&src) {
            Ok(m) => m,
            Err(errs) => {
                p.status = PluginStatus::Error(errs.join("\n"));
                return p;
            }
        };
        if self.plugins.iter().any(|o| o.dir != p.dir && o.manifest.as_ref().is_some_and(|om| om.id == manifest.id)) {
            p.status = PluginStatus::Error(format!("another plugin already uses id \"{}\"", manifest.id));
            p.manifest = Some(manifest);
            return p;
        }
        let script = match std::fs::read_to_string(dir.join(&manifest.entry)) {
            Ok(s) => s,
            Err(e) => {
                p.status = PluginStatus::Error(format!("cannot read {}: {e}", manifest.entry));
                p.manifest = Some(manifest);
                return p;
            }
        };
        match self.engine.compile(&script) {
            Ok(ast) => {
                p.hooks = ast.iter_functions().map(|f| (f.name.to_string(), f.params.len())).collect();
                p.ast = Some(ast);
            }
            Err(e) => {
                p.status = PluginStatus::Error(format!("{}: {e}", manifest.entry));
                p.manifest = Some(manifest);
                return p;
            }
        }
        let settings = self.settings_map(&manifest);
        let store = std::fs::read_to_string(dir.join("store.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| json_to_dynamic(&v).try_cast::<Map>())
            .unwrap_or_default();
        p.ctx = Some(Ctx::new(&manifest.id, manifest.permission_set(), settings, store));
        p.manifest = Some(manifest);
        p
    }

    fn settings_map(&self, m: &Manifest) -> Map {
        let overrides = self.state.settings.get(&m.id);
        let mut map = Map::new();
        for (k, spec) in &m.settings {
            let mut v = toml_to_dynamic(&spec.default);
            if spec.kind == SettingKind::Float {
                if let Ok(i) = v.as_int() {
                    v = (i as f64).into();
                }
            }
            if let Some(o) = overrides.and_then(|o| o.get(k)) {
                let d = json_to_dynamic(o);
                if d.type_name() == v.type_name() || (spec.kind == SettingKind::Float && d.is::<i64>()) {
                    v = if spec.kind == SettingKind::Float { d.as_float().or(d.as_int().map(|i| i as f64)).unwrap_or(0.0).into() } else { d };
                }
            }
            map.insert(k.as_str().into(), v);
        }
        map
    }

    fn approved(&self, m: &Manifest) -> bool {
        let granted = self.state.granted.get(&m.id);
        m.permissions.iter().all(|p| granted.is_some_and(|g| g.contains(p)))
    }

    fn start_all_pending(&mut self) {
        for i in 0..self.plugins.len() {
            self.start_at(i);
        }
    }

    /// Decide status for a freshly loaded plugin and run `on_load` if allowed.
    fn start_at(&mut self, i: usize) {
        if matches!(self.plugins[i].status, PluginStatus::Error(_)) {
            return;
        }
        let Some(m) = self.plugins[i].manifest.clone() else { return };
        self.plugins[i].status = match self.state.enabled.get(&m.id) {
            Some(false) => PluginStatus::Disabled,
            Some(true) if self.approved(&m) => PluginStatus::Running,
            _ => PluginStatus::NeedsApproval,
        };
        if self.plugins[i].status == PluginStatus::Running {
            self.call(i, "on_load", vec![]);
        }
    }

    fn unload_at(&mut self, i: usize) {
        if self.plugins[i].status == PluginStatus::Running {
            self.call(i, "on_unload", vec![]);
        }
        let id = self.plugins[i].id();
        if self.overlay.as_ref().is_some_and(|s| s.plugin_id == id) {
            self.overlay = None;
            self.limiter.reset();
        }
        self.flush_store(i);
    }

    fn reload_at(&mut self, i: usize) {
        self.unload_at(i);
        let dir = self.plugins[i].dir.clone();
        let mut fresh = self.load_dir(&dir);
        fresh.push_log("reloaded".into());
        self.plugins[i] = fresh;
        self.start_at(i);
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.plugins.iter().position(|p| p.id() == id)
    }

    pub fn get(&self, id: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.id() == id)
    }

    pub fn reload(&mut self, id: &str) {
        if let Some(i) = self.index_of(id) {
            self.reload_at(i);
        }
    }

    /// Grant every permission the plugin asks for and enable it.
    pub fn approve(&mut self, id: &str) {
        let Some(i) = self.index_of(id) else { return };
        let Some(m) = self.plugins[i].manifest.clone() else { return };
        self.state.granted.insert(m.id.clone(), m.permissions.clone());
        self.state.enabled.insert(m.id.clone(), true);
        self.save_state();
        self.reload_at(i);
    }

    pub fn set_enabled(&mut self, id: &str, on: bool) {
        let Some(i) = self.index_of(id) else { return };
        self.state.enabled.insert(id.to_string(), on);
        self.save_state();
        if on {
            self.reload_at(i);
        } else {
            self.unload_at(i);
            if !matches!(self.plugins[i].status, PluginStatus::Error(_)) {
                self.plugins[i].status = PluginStatus::Disabled;
            }
        }
    }

    /// Change one user setting (persisted) and update the live `ctx.settings`.
    pub fn set_setting(&mut self, id: &str, key: &str, value: serde_json::Value) {
        self.state.settings.entry(id.to_string()).or_default().insert(key.to_string(), value);
        self.save_state();
        if let Some(i) = self.index_of(id) {
            if let Some(m) = self.plugins[i].manifest.clone() {
                let map = self.settings_map(&m);
                if let Some(ctx) = &self.plugins[i].ctx {
                    ctx.0.borrow_mut().settings = map;
                }
            }
        }
    }

    pub fn setting(&self, id: &str, key: &str) -> Option<Dynamic> {
        let i = self.index_of(id)?;
        self.plugins[i].ctx.as_ref()?.0.borrow().settings.get(key).cloned()
    }

    fn flush_store(&mut self, i: usize) {
        if self.readonly {
            return;
        }
        let p = &self.plugins[i];
        if let Some(ctx) = &p.ctx {
            let mut d = ctx.0.borrow_mut();
            if d.store_dirty {
                d.store_dirty = false;
                if let Ok(v) = rhai::serde::from_dynamic::<serde_json::Value>(&Dynamic::from_map(d.store.clone())) {
                    let _ = std::fs::write(p.dir.join("store.json"), v.to_string());
                }
            }
        }
    }

    /// Call a hook with `ctx` prepended. Returns `None` if the hook is absent,
    /// not permitted, or failed (failures are logged and counted).
    fn call(&mut self, i: usize, hook: &str, args: Vec<Dynamic>) -> Option<Dynamic> {
        let flash_cap = self.flash_cap();
        let now = self.now();
        let overlay = self.overlay.clone();
        let p = &mut self.plugins[i];
        if !matches!(p.status, PluginStatus::Running) {
            return None;
        }
        let arity = args.len() + 1;
        if !p.hooks.iter().any(|(n, a)| n == hook && *a == arity) {
            return None;
        }
        if let Some((_, _, Some(perm))) = HOOKS.iter().find(|(n, _, _)| *n == hook) {
            if !p.has_perm(*perm) {
                return None;
            }
        }
        let ctx = p.ctx.clone()?;
        {
            let mut d = ctx.0.borrow_mut();
            d.now = now;
            d.flash_cap = flash_cap;
            let mine = overlay.as_ref().filter(|s| s.plugin_id == d.plugin_id);
            d.overlay_active = mine.is_some_and(|s| s.phase == OverlayPhase::Running);
            d.fullscreen = mine.is_some_and(|s| s.fullscreen);
        }
        let mut full = Vec::with_capacity(arity);
        full.push(Dynamic::from(ctx.clone()));
        full.extend(args);
        let ast = p.ast.as_ref()?;
        let mut scope = Scope::new();
        let options = CallFnOptions::new().eval_ast(false).rewind_scope(true);
        let engine = &self.engine;
        let result = with_deadline(HOOK_BUDGET, || engine.call_fn_with_options::<Dynamic>(options, &mut scope, ast, hook, full));

        let (logs, reqs) = {
            let mut d = ctx.0.borrow_mut();
            (std::mem::take(&mut d.logs), std::mem::take(&mut d.requests))
        };
        let id = p.id();
        for l in logs {
            p.push_log(l);
        }
        match result {
            Ok(v) => {
                for r in reqs {
                    self.requests.push((id.clone(), r));
                }
                Some(v)
            }
            Err(e) => {
                p.errors += 1;
                let pos = e.position();
                let mut msg = format!("{hook}: {e}");
                if let (Some(line), false) = (pos.line(), msg.contains("(line ")) {
                    msg.push_str(&format!(" (line {line}, position {})", pos.position().unwrap_or(0)));
                }
                p.push_log(format!("error: {msg}"));
                if p.errors >= MAX_ERRORS {
                    p.status = PluginStatus::Error(format!("stopped after {MAX_ERRORS} errors; last: {msg}"));
                    if self.overlay.as_ref().is_some_and(|s| s.plugin_id == id) {
                        self.overlay = None;
                        self.limiter.reset();
                    }
                }
                None
            }
        }
    }

    fn each_running(&mut self, hook: &str, args: impl Fn() -> Vec<Dynamic>) {
        for i in 0..self.plugins.len() {
            if self.plugins[i].status == PluginStatus::Running {
                self.call(i, hook, args());
            }
        }
    }

    // ---- events from the app ------------------------------------------------------

    pub fn station_changed(&mut self, s: &StationInfo) {
        let mut m = Map::new();
        m.insert("name".into(), s.name.clone().into());
        m.insert("url".into(), s.url.clone().into());
        m.insert("tags".into(), s.tags.clone().into());
        m.insert("country".into(), s.country.clone().into());
        m.insert("codec".into(), s.codec.clone().into());
        m.insert("bitrate".into(), (s.bitrate as i64).into());
        self.each_running("on_station_change", || vec![Dynamic::from_map(m.clone())]);
    }

    pub fn meta(&mut self, title: &str) {
        let t: Dynamic = title.to_string().into();
        self.each_running("on_meta", || vec![t.clone()]);
    }

    /// `state` is "connecting" | "playing" | "stopped" | "failed".
    pub fn player(&mut self, state: &str) {
        let t: Dynamic = state.to_string().into();
        self.each_running("on_player", || vec![t.clone()]);
    }

    /// Per-frame: `on_audio` then `on_tick(dt)`.
    pub fn frame(&mut self, dt: f64, audio: &AudioFrame) {
        let f = audio.features;
        let mut a = Map::new();
        a.insert("level".into(), (f.level as f64).into());
        a.insert("peak".into(), (f.peak as f64).into());
        a.insert("bass".into(), (f.bass as f64).into());
        a.insert("mid".into(), (f.mid as f64).into());
        a.insert("treble".into(), (f.treble as f64).into());
        a.insert("beat".into(), f.beat.into());
        a.insert("bpm".into(), (f.bpm as f64).into());
        a.insert("quiet".into(), f.quiet.into());
        a.insert("playing".into(), audio.playing.into());
        let bands: rhai::Array = audio.bands.iter().map(|b| Dynamic::from(*b as f64)).collect();
        a.insert("bands".into(), bands.into());
        let a = Dynamic::from_map(a);
        self.each_running("on_audio", || vec![a.clone()]);
        let dt: Dynamic = dt.into();
        self.each_running("on_tick", || vec![dt.clone()]);
        // Drop the session if its owner went away.
        if let Some(s) = &self.overlay {
            if self.get(&s.plugin_id).is_none_or(|p| p.status != PluginStatus::Running) {
                self.overlay = None;
                self.limiter.reset();
            }
        }
    }

    /// Requests from plugins since the last call. Overlay requests are
    /// handled here (they start the warning screen); the rest go to the app.
    pub fn take_requests(&mut self) -> Vec<(String, Request)> {
        let reqs = std::mem::take(&mut self.requests);
        let mut out = Vec::new();
        for (id, r) in reqs {
            match r {
                Request::OpenOverlay { fullscreen } => {
                    let busy = self.overlay.as_ref().is_some_and(|s| s.plugin_id != id);
                    if !busy {
                        let from_hotkey = self.hotkey_plugin.as_deref() == Some(id.as_str());
                        let calm = self.get(&id).and_then(|p| p.manifest.as_ref()).is_some_and(|m| !m.flashing);
                        let phase = match &self.overlay {
                            Some(s) if s.plugin_id == id => s.phase,
                            _ if from_hotkey || calm => OverlayPhase::Running,
                            _ => OverlayPhase::Warning,
                        };
                        self.overlay = Some(OverlaySession { plugin_id: id, fullscreen, phase });
                        self.limiter.reset();
                    }
                }
                Request::CloseOverlay => {
                    if self.overlay.as_ref().is_some_and(|s| s.plugin_id == id) {
                        self.end_overlay();
                    }
                }
                other => out.push((id, other)),
            }
        }
        out
    }

    /// User accepted the photosensitivity warning.
    pub fn accept_warning(&mut self) {
        if let Some(s) = &mut self.overlay {
            s.phase = OverlayPhase::Running;
            self.limiter.reset();
        }
    }

    /// Close the overlay (Esc, any other key, click, or plugin request).
    pub fn end_overlay(&mut self) {
        if let Some(s) = self.overlay.take() {
            self.limiter.reset();
            if let Some(i) = self.index_of(&s.plugin_id) {
                self.call(i, "on_overlay_closed", vec![]);
            }
            // A plugin re-opening from on_overlay_closed is ignored.
            self.requests.retain(|(_, r)| !matches!(r, Request::OpenOverlay { .. }));
        }
    }

    /// A key while the overlay is open: "Escape", "ArrowUp", "A", ...,
    /// or "MouseClick". Esc is handled by the host before the plugin sees it.
    pub fn key(&mut self, key: &str) {
        let Some(s) = self.overlay.clone() else { return };
        if key == "Escape" {
            self.end_overlay();
            return;
        }
        match s.phase {
            OverlayPhase::Warning => {
                if key == "Enter" {
                    self.accept_warning();
                } else if key != "MouseClick" {
                    self.end_overlay();
                }
            }
            OverlayPhase::Running => {
                let consumed = self
                    .index_of(&s.plugin_id)
                    .and_then(|i| self.call(i, "on_key", vec![key.to_string().into()]))
                    .and_then(|v| v.as_bool().ok())
                    .unwrap_or(false);
                if !consumed {
                    self.end_overlay();
                }
            }
        }
    }

    /// Draw commands for the running overlay (already safety-filtered).
    pub fn overlay_commands(&mut self) -> Vec<DrawCmd> {
        let Some(s) = self.overlay.clone() else { return Vec::new() };
        if s.phase != OverlayPhase::Running {
            return Vec::new();
        }
        let Some(i) = self.index_of(&s.plugin_id) else { return Vec::new() };
        let draw = Draw::default();
        let ok = self.call(i, "on_overlay", vec![Dynamic::from(draw.clone())]).is_some();
        let mut cmds = std::mem::take(&mut *draw.0.borrow_mut());
        if !ok {
            cmds = vec![DrawCmd::Fill(crate::Rgba::BLACK)];
        }
        let now = self.now();
        self.limiter.filter(now, &mut cmds);
        cmds
    }

    /// Hotkeys of running plugins: (plugin id, spec), in a stable order.
    pub fn hotkeys(&self) -> Vec<(String, crate::HotkeySpec)> {
        self.plugins
            .iter()
            .filter(|p| p.status == PluginStatus::Running && p.has_perm(Permission::Hotkeys))
            .flat_map(|p| p.manifest.iter().flat_map(|m| m.hotkeys.iter().map(|h| (m.id.clone(), h.clone()))))
            .collect()
    }

    /// Feed a key press (outside overlays). Returns true if it belongs to a
    /// plugin hotkey and must not reach the rest of the app. A completed
    /// chord calls `on_hotkey(ctx, action)` and its requests are applied
    /// immediately; an overlay opened there skips the warning screen
    /// because the user started it on purpose.
    pub fn hotkey_key(&mut self, key: &str, mods: crate::Mods) -> bool {
        if self.overlay.is_some() {
            return false;
        }
        let list = self.hotkeys();
        let chords: Vec<Vec<crate::Step>> = list.iter().map(|(_, h)| crate::parse_chord(&h.keys).unwrap_or_default()).collect();
        let now = self.now();
        match self.chords.key(&chords, now, key, mods) {
            crate::ChordResult::Ignored => false,
            crate::ChordResult::Pending => true,
            crate::ChordResult::Fired(i) => {
                let (id, spec) = list[i].clone();
                if let Some(idx) = self.index_of(&id) {
                    self.hotkey_plugin = Some(id.clone());
                    self.call(idx, "on_hotkey", vec![spec.action.into()]);
                    // Apply the overlay request while the grant is active.
                    let others = self.take_requests();
                    self.hotkey_plugin = None;
                    self.requests.extend(others);
                }
                true
            }
        }
    }

    /// Programmatically fire a plugin's `on_hotkey(ctx, action)` hook, as if
    /// the user had pressed the hotkey bound to that action. Used by the
    /// idle-Visions screensaver. Returns false when the plugin is not
    /// running or an overlay is already open. Overlay requests made by the
    /// hook skip the warning screen (the user opted in via Settings).
    pub fn trigger_action(&mut self, plugin_id: &str, action: &str) -> bool {
        if self.overlay.is_some() {
            return false;
        }
        let Some(idx) = self.index_of(plugin_id) else { return false };
        if self.plugins[idx].status != PluginStatus::Running {
            return false;
        }
        self.hotkey_plugin = Some(plugin_id.to_string());
        self.call(idx, "on_hotkey", vec![action.into()]);
        let others = self.take_requests();
        self.hotkey_plugin = None;
        self.requests.extend(others);
        true
    }

    /// Pages contributed by running plugins: (plugin id, page).
    pub fn pages(&self) -> Vec<(String, PageSpec)> {
        self.plugins
            .iter()
            .filter(|p| p.status == PluginStatus::Running && p.has_perm(Permission::Page))
            .flat_map(|p| p.manifest.iter().flat_map(move |m| m.pages.iter().map(move |pg| (m.id.clone(), pg.clone()))))
            .collect()
    }

    fn surface(&mut self, id: &str, surface: &str, hook: &str, extra: Vec<Dynamic>) -> Vec<Widget> {
        let Some(i) = self.index_of(id) else { return Vec::new() };
        let results = self.plugins[i].ui_results.remove(surface).unwrap_or_default();
        let ui = Ui::new(results);
        let mut args = extra;
        args.push(Dynamic::from(ui.clone()));
        self.call(i, hook, args);
        ui.finish()
    }

    /// Widgets for the plugin's panel on the Plugins page.
    pub fn panel(&mut self, id: &str) -> Vec<Widget> {
        self.surface(id, "panel", "on_panel", vec![])
    }

    /// Widgets for a whole nav page declared in `[[pages]]`.
    pub fn page(&mut self, id: &str, page_id: &str) -> Vec<Widget> {
        self.surface(id, &format!("page:{page_id}"), "on_page", vec![page_id.to_string().into()])
    }

    /// Report widget interactions (applied on the next frame).
    pub fn set_ui_results(&mut self, id: &str, surface: &str, results: HashMap<usize, UiResult>) {
        if let Some(i) = self.index_of(id) {
            if !results.is_empty() {
                self.plugins[i].ui_results.insert(surface.to_string(), results);
            }
        }
    }

    /// Simulate the call sequence used by `--check-plugin`.
    pub(crate) fn smoke_test(&mut self, id: &str) -> Vec<String> {
        let Some(i) = self.index_of(id) else { return vec!["plugin not loaded".into()] };
        let before = self.plugins[i].log.len();
        let audio = AudioFrame {
            features: bwr_spectrum::AudioFeatures { level: 0.4, peak: 0.8, bass: 0.7, mid: 0.4, treble: 0.2, beat: true, bpm: 120.0, quiet: false },
            bands: [0.5; bwr_spectrum::BANDS],
            playing: true,
        };
        self.station_changed(&StationInfo { name: "Test FM".into(), url: "http://example.invalid/stream".into(), ..Default::default() });
        self.player("playing");
        self.meta("Artist - Song");
        for _ in 0..3 {
            self.frame(1.0 / 60.0, &audio);
        }
        let _ = self.panel(id);
        if let Some(m) = self.plugins[i].manifest.clone() {
            for pg in &m.pages {
                let _ = self.page(id, &pg.id);
            }
        }
        let _ = self.take_requests();
        if self.overlay.is_some() {
            self.accept_warning();
        } else if self.plugins[i].has_perm(Permission::Overlay) {
            self.overlay = Some(OverlaySession { plugin_id: id.to_string(), fullscreen: false, phase: OverlayPhase::Running });
        }
        let _ = self.overlay_commands();
        self.frame(1.0 / 60.0, &audio);
        let _ = self.overlay_commands();
        if self.overlay.is_some() {
            self.key("ArrowUp");
        }
        if self.overlay.is_some() {
            self.key("Escape");
        }
        if self.overlay.is_some() {
            self.end_overlay();
        }
        if let Some(m) = self.plugins[i].manifest.clone() {
            for h in &m.hotkeys {
                self.call(i, "on_hotkey", vec![h.action.clone().into()]);
                let _ = self.take_requests();
                let _ = self.overlay_commands();
                self.end_overlay();
            }
        }
        self.unload_at(i);
        let mut errs: Vec<String> = Vec::new();
        for l in self.plugins[i].log.iter().skip(before) {
            if let Some(e) = l.strip_prefix("error: ") {
                if !errs.iter().any(|x| x == e) {
                    errs.push(e.to_string());
                }
            }
        }
        errs
    }

    /// Call any hook directly (tests and tooling).
    #[doc(hidden)]
    pub fn call_raw(&mut self, id: &str, hook: &str, args: Vec<Dynamic>) -> Option<Dynamic> {
        let i = self.index_of(id)?;
        self.call(i, hook, args)
    }
}
