use std::path::{Path, PathBuf};

use bwr_plugins::*;
use bwr_plugins::Manifest;

fn write_plugin(root: &Path, id: &str, perms: &str, script: &str) -> PathBuf {
    let dir = root.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("plugin.toml"),
        format!("id = \"{id}\"\nname = \"Test {id}\"\nversion = \"0.1.0\"\napi = 1\npermissions = [{perms}]\n"),
    )
    .unwrap();
    std::fs::write(dir.join("main.rhai"), script).unwrap();
    dir
}

fn bundled_strobe() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/strobe")
}

const COUNTER: &str = r#"
fn on_load(ctx) { ctx.state.n = 0; ctx.state.title = ""; }
fn on_tick(ctx, dt) { ctx.state.n += 1; }
fn on_meta(ctx, t) { ctx.state.title = t; }
fn get(ctx) { ctx.state }
"#;

#[test]
fn new_plugins_need_approval_then_run() {
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(tmp.path(), "counter", "\"now_playing\"", COUNTER);
    let mut host = PluginHost::new(tmp.path());
    assert_eq!(host.plugins.len(), 1);
    assert_eq!(host.plugins[0].status, PluginStatus::NeedsApproval);
    host.frame(0.016, &AudioFrame::default());
    assert!(host.call_raw("counter", "get", vec![]).is_none(), "not running yet");

    host.approve("counter");
    assert_eq!(host.plugins[0].status, PluginStatus::Running);
    host.frame(0.016, &AudioFrame::default());
    host.frame(0.016, &AudioFrame::default());
    host.meta("Artist - Song");
    let st = host.call_raw("counter", "get", vec![]).unwrap().cast::<rhai::Map>();
    assert_eq!(st["n"].as_int().unwrap(), 2, "chained ctx.state mutation persists");
    assert_eq!(st["title"].clone().into_string().unwrap(), "Artist - Song");

    // Approval persists across hosts.
    let host2 = PluginHost::new(tmp.path());
    assert_eq!(host2.plugins[0].status, PluginStatus::Running);

    // Disable sticks too.
    let mut host3 = host2;
    host3.set_enabled("counter", false);
    assert_eq!(PluginHost::new(tmp.path()).plugins[0].status, PluginStatus::Disabled);
}

#[test]
fn hooks_are_gated_by_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    // No now_playing permission: on_meta must never run.
    write_plugin(tmp.path(), "counter", "", COUNTER);
    let mut host = PluginHost::new(tmp.path());
    host.approve("counter");
    host.meta("secret");
    let st = host.call_raw("counter", "get", vec![]).unwrap().cast::<rhai::Map>();
    assert_eq!(st["title"].clone().into_string().unwrap(), "");
}

#[test]
fn api_calls_without_permission_fail_and_errors_auto_stop() {
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(tmp.path(), "rude", "", "fn on_tick(ctx, dt) { ctx.player_stop(); }");
    let mut host = PluginHost::new(tmp.path());
    host.approve("rude");
    for _ in 0..5 {
        host.frame(0.016, &AudioFrame::default());
    }
    match &host.plugins[0].status {
        PluginStatus::Error(e) => assert!(e.contains("player_control"), "{e}"),
        s => panic!("expected auto-stop, got {s:?}"),
    }
    assert!(host.take_requests().is_empty());
}

#[test]
fn runaway_scripts_are_stopped() {
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(tmp.path(), "spin", "", "fn on_tick(ctx, dt) { loop { } }");
    let mut host = PluginHost::new(tmp.path());
    host.approve("spin");
    let t = std::time::Instant::now();
    host.frame(0.016, &AudioFrame::default());
    assert!(t.elapsed() < std::time::Duration::from_secs(2));
    assert!(host.plugins[0].log.iter().any(|l| l.starts_with("error:")));
}

#[test]
fn hot_reload_picks_up_edits_and_new_folders() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = write_plugin(tmp.path(), "counter", "", COUNTER);
    let mut host = PluginHost::new(tmp.path());
    host.approve("counter");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(dir.join("main.rhai"), format!("{COUNTER}\nfn version(ctx) {{ 2 }}")).unwrap();
    write_plugin(tmp.path(), "second", "", "fn on_load(ctx) {}");
    host.poll_changes();
    assert_eq!(host.call_raw("counter", "version", vec![]).unwrap().as_int().unwrap(), 2);
    assert_eq!(host.plugins.len(), 2);

    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::remove_dir_all(tmp.path().join("second")).unwrap();
    host.poll_changes();
    assert_eq!(host.plugins.len(), 1);
}

#[test]
fn bad_plugins_report_errors() {
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(tmp.path(), "broken", "", "fn on_load(ctx) {\n  let x = ;\n}");
    write_plugin(tmp.path(), "netty", "\"net\"", "");
    let host = PluginHost::new(tmp.path());
    for p in &host.plugins {
        assert!(matches!(p.status, PluginStatus::Error(_)), "{:?}", p.status);
    }
    let broken = host.get("broken").unwrap();
    let PluginStatus::Error(e) = &broken.status else { unreachable!() };
    assert!(e.contains("line 2"), "{e}");
}

const OVERLAY: &str = r##"
fn on_load(ctx) { ctx.state.keys = []; }
fn on_panel(ctx, ui) { if ui.button("go") { ctx.open_overlay(true); } }
fn on_overlay(ctx, draw) { draw.fill("#ffffff"); draw.svg("M0 0 L100 100", 0.5, 0.5, 0.5, 2, "#ff0000"); }
fn on_key(ctx, key) {
    if key == "ArrowUp" { ctx.state.keys.push(key); return true; }
    if key == "Hang" { loop {} }
    false
}
fn keys(ctx) { ctx.state.keys }
"##;

fn overlay_host(tmp: &Path) -> PluginHost {
    write_plugin(tmp, "ov", "\"overlay\", \"fullscreen\", \"hotkeys\", \"panel\"", OVERLAY);
    let mut host = PluginHost::new(tmp);
    host.approve("ov");
    // Click "go" (widget 0) and run the next panel frame.
    let w = host.panel("ov");
    assert_eq!(w, vec![Widget::Button("go".into())]);
    host.set_ui_results("ov", "panel", [(0usize, UiResult::Clicked)].into_iter().collect());
    host.panel("ov");
    assert!(host.take_requests().is_empty(), "overlay requests are handled by the host");
    host
}

#[test]
fn overlay_starts_with_warning_and_esc_always_exits() {
    let tmp = tempfile::tempdir().unwrap();
    let mut host = overlay_host(tmp.path());
    let s = host.overlay.clone().expect("session opened");
    assert!(s.fullscreen);
    assert_eq!(s.phase, OverlayPhase::Warning);
    assert!(host.overlay_commands().is_empty(), "nothing from the plugin during the warning");

    host.key("Enter");
    assert_eq!(host.overlay.as_ref().unwrap().phase, OverlayPhase::Running);
    let cmds = host.overlay_commands();
    assert!(matches!(cmds[0], DrawCmd::Fill(_)));
    assert!(matches!(cmds[1], DrawCmd::Paths { .. }));

    host.key("ArrowUp");
    assert!(host.overlay.is_some(), "arrow keys are consumed by the plugin");
    let keys = host.call_raw("ov", "keys", vec![]).unwrap().cast::<rhai::Array>();
    assert_eq!(keys.len(), 1);

    host.key("Escape");
    assert!(host.overlay.is_none());
}

#[test]
fn other_keys_clicks_and_hung_plugins_exit() {
    for key in ["Q", "MouseClick", "Hang"] {
        let tmp = tempfile::tempdir().unwrap();
        let mut host = overlay_host(tmp.path());
        host.key("Enter");
        host.key(key);
        assert!(host.overlay.is_none(), "{key} should close the overlay");
    }
    // During the warning, any key except Enter cancels.
    let tmp = tempfile::tempdir().unwrap();
    let mut host = overlay_host(tmp.path());
    host.key("X");
    assert!(host.overlay.is_none());
}

#[test]
fn flash_cap_is_enforced_by_host() {
    let tmp = tempfile::tempdir().unwrap();
    let mut host = PluginHost::new(tmp.path());
    assert_eq!(host.flash_cap(), DEFAULT_FLASH_HZ);
    host.set_flash_safety(false, 12.0);
    assert_eq!(host.flash_cap(), 3.0, "no opt-in: stays at 3 Hz");
    host.set_flash_safety(true, 50.0);
    assert_eq!(host.flash_cap(), HARD_MAX_FLASH_HZ, "opt-in: hard ceiling 12 Hz");
}

#[test]
fn check_plugin_accepts_bundled_strobe() {
    let r = check_plugin(&bundled_strobe());
    assert!(r.ok(), "{r}");
    for h in ["on_audio", "on_overlay", "on_key", "on_panel", "on_page"] {
        assert!(r.hooks.iter().any(|x| x == h), "{h} missing: {r}");
    }
}

#[test]
fn check_plugin_reports_problems() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = write_plugin(tmp.path(), "bad", "\"audio\"", "fn on_audio(ctx) {}\nfn on_frobnicate(ctx) {}");
    let r = check_plugin(&dir);
    assert!(!r.ok());
    let text = r.to_string();
    assert!(text.contains("on_audio takes 2"), "{text}");
    assert!(text.contains("on_frobnicate"), "{text}");
    let dir = write_plugin(tmp.path(), "crashy", "\"storage\"", "fn on_load(ctx) { ctx.store_set(\"k\", 1); }\nfn on_tick(ctx, dt) {\n  let y = no_such_fn(dt);\n}");
    let r = check_plugin(&dir);
    let text = r.to_string();
    assert!(text.contains("runtime: on_tick") && text.contains("line 3"), "{text}");
    assert!(!dir.join("store.json").exists(), "check never writes into the plugin");

    let r = check_plugin(&tmp.path().join("missing"));
    assert!(!r.ok());
}

#[test]
fn strobe_flashes_on_beats_and_blacks_out_in_breaks() {
    let tmp = tempfile::tempdir().unwrap();
    install_bundled(tmp.path());
    let mut host = PluginHost::new(tmp.path());
    host.approve("strobe");
    host.panel("strobe");
    // "Start strobe" is the 5th widget (label, space, mode, display, speed, small, space, button).
    let w = host.panel("strobe");
    let start = w.iter().position(|w| *w == Widget::Button("Start strobe".into())).unwrap();
    host.set_ui_results("strobe", "panel", [(start, UiResult::Clicked)].into_iter().collect());
    host.panel("strobe");
    let _ = host.take_requests();
    assert!(host.overlay.is_some());
    host.key("Enter");

    let mut beat = AudioFrame { playing: true, ..Default::default() };
    beat.features.level = 0.5;
    beat.features.bass = 0.8;
    beat.features.quiet = false;
    let mut bright_frames = 0;
    for i in 0..600 {
        beat.features.beat = i % 30 == 0; // 120 BPM at 60 fps
        host.frame(1.0 / 60.0, &beat);
        let cmds = host.overlay_commands();
        if cmds.iter().any(|c| matches!(c, DrawCmd::Fill(col) if col.luminance() > 0.5)) {
            bright_frames += 1;
        }
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
    assert!(bright_frames > 0, "strobe never flashed");
    assert!(host.plugins[0].log.iter().all(|l| !l.starts_with("error")), "{:?}", host.plugins[0].log);

    // Break: quiet audio -> black.
    beat.features.quiet = true;
    beat.features.beat = false;
    host.frame(1.0 / 60.0, &beat);
    let cmds = host.overlay_commands();
    assert_eq!(cmds, vec![DrawCmd::Fill(Rgba::BLACK)]);

    host.key("ArrowRight");
    assert!(host.overlay.is_some());
    host.key("k");
    assert!(host.overlay.is_none());
}

#[test]
fn zip_import_installs_and_rejects_bad_archives() {
    use std::io::Write;
    let tmp = tempfile::tempdir().unwrap();
    let make = |name: &str, files: &[(&str, &str)]| {
        let p = tmp.path().join(name);
        let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
        for (n, body) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
        p
    };
    let toml = "id = \"zipped\"\nname = \"Zipped\"\nversion = \"1.0.0\"\napi = 1\n";
    let plugins = tmp.path().join("plugins");
    let good = make("good.zip", &[("zipped-main/plugin.toml", toml), ("zipped-main/main.rhai", "fn on_load(ctx) {}")]);
    assert_eq!(import_zip(&good, &plugins).unwrap(), "zipped");
    assert!(plugins.join("zipped/main.rhai").is_file());
    // Re-import replaces.
    assert_eq!(import_zip(&good, &plugins).unwrap(), "zipped");

    let evil = make("evil.zip", &[("plugin.toml", toml), ("../escape.txt", "x"), ("main.rhai", "")]);
    assert!(import_zip(&evil, &plugins).is_err());
    assert!(!tmp.path().join("escape.txt").exists());

    let broken = make("broken.zip", &[("plugin.toml", toml), ("main.rhai", "fn (")]);
    assert!(import_zip(&broken, &plugins).unwrap_err().contains("failed checks"));

    let empty = make("empty.zip", &[("readme.txt", "hi")]);
    assert!(import_zip(&empty, &plugins).is_err());
    // No staging folders left behind.
    let leftovers: Vec<_> = std::fs::read_dir(&plugins).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with(".import")).collect();
    assert!(leftovers.is_empty());
}

#[test]
fn install_bundled_never_overwrites() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(install_bundled(tmp.path()), vec!["strobe".to_string(), "visions".to_string()]);
    std::fs::write(tmp.path().join("strobe/main.rhai"), "// mine").unwrap();
    assert!(install_bundled(tmp.path()).is_empty());
    assert_eq!(std::fs::read_to_string(tmp.path().join("strobe/main.rhai")).unwrap(), "// mine");
    assert!(tmp.path().join("PLUGIN_GUIDE.md").is_file());
}

#[test]
fn guide_examples_pass_check() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/plugin-examples");
    let mut n = 0;
    for e in std::fs::read_dir(root).unwrap().flatten() {
        let r = check_plugin(&e.path());
        assert!(r.ok() && r.warnings.is_empty(), "{}: {r}", e.path().display());
        n += 1;
    }
    assert!(n >= 4);
}

#[test]
fn guide_inlines_the_example_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let guide = std::fs::read_to_string(root.join("docs/PLUGIN_GUIDE.md")).unwrap();
    for e in std::fs::read_dir(root.join("docs/plugin-examples")).unwrap().flatten() {
        for f in ["plugin.toml", "main.rhai"] {
            let body = std::fs::read_to_string(e.path().join(f)).unwrap();
            assert!(guide.contains(body.trim()), "PLUGIN_GUIDE.md is out of date for {}/{f}", e.path().display());
        }
    }
}

fn strobe_host(tmp: &Path) -> PluginHost {
    install_bundled(tmp);
    let mut host = PluginHost::new(tmp);
    host.approve("strobe");
    host
}

const CTRL: Mods = Mods { ctrl: true, shift: false, alt: false };

#[test]
fn strobe_hotkey_starts_immediately_without_warning() {
    let tmp = tempfile::tempdir().unwrap();
    let mut host = strobe_host(tmp.path());
    assert_eq!(host.hotkeys().len(), 1);
    assert_eq!(host.hotkeys()[0].1.keys, "Ctrl+F,U");

    assert!(!host.hotkey_key("U", Mods::default()), "plain U is not swallowed");
    assert!(host.hotkey_key("F", CTRL), "Ctrl+F is swallowed (chord pending)");
    assert!(host.overlay.is_none());
    assert!(host.hotkey_key("U", CTRL));
    let s = host.overlay.clone().expect("strobe started");
    assert_eq!(s.phase, OverlayPhase::Running, "hotkey skips the warning screen");
    assert!(s.fullscreen, "default display is fullscreen");
    assert!(!host.hotkey_key("F", CTRL), "no chords while the overlay is open");

    host.key("Escape");
    assert!(host.overlay.is_none(), "Esc always exits");

    // Other keys and clicks still exit.
    for k in ["Q", "MouseClick"] {
        host.hotkey_key("F", CTRL);
        host.hotkey_key("U", Mods::default());
        assert!(host.overlay.is_some());
        host.key(k);
        assert!(host.overlay.is_none(), "{k}");
    }
}

#[test]
fn idle_trigger_action_starts_visions_without_hotkey() {
    let tmp = tempfile::tempdir().unwrap();
    let mut host = strobe_host(tmp.path());
    host.approve("visions");

    assert!(!host.trigger_action("visions", "start") || host.overlay.is_some());
    let s = host.overlay.clone().expect("visions started");
    assert_eq!(s.plugin_id, "visions");
    assert_eq!(s.phase, OverlayPhase::Running, "non-flashing plugin skips the warning screen");
    assert!(s.fullscreen, "default display is fullscreen");

    // No stacking: a second trigger while open is a no-op, and unknown
    // plugin ids are reported as failures.
    assert!(!host.trigger_action("visions", "start"), "already open");
    assert!(!host.trigger_action("nope", "start"), "unknown plugin");

    host.end_overlay();
    assert!(host.overlay.is_none());
    assert!(host.trigger_action("visions", "start"), "re-arms after close");
}

#[test]
fn panel_start_still_shows_warning() {
    let tmp = tempfile::tempdir().unwrap();
    let mut host = strobe_host(tmp.path());
    let w = host.panel("strobe");
    let start = w.iter().position(|w| *w == Widget::Button("Start strobe".into())).unwrap();
    host.set_ui_results("strobe", "panel", [(start, UiResult::Clicked)].into_iter().collect());
    host.panel("strobe");
    let _ = host.take_requests();
    assert_eq!(host.overlay.as_ref().unwrap().phase, OverlayPhase::Warning);
}

#[test]
fn strobe_defaults_fixed_12hz_but_clamped_to_cap() {
    let count_onsets = |host: &mut PluginHost| {
        let mut onsets = 0;
        let mut prev = false;
        let t0 = std::time::Instant::now();
        let mut last = t0;
        while t0.elapsed() < std::time::Duration::from_secs(2) {
            // Real frame time, like the app: the flash limiter runs on the wall
            // clock, so a fixed dt drifts against it when sleep() overshoots (macOS).
            let now = std::time::Instant::now();
            host.frame(now.duration_since(last).as_secs_f64().max(1e-4) as _, &AudioFrame::default());
            last = now;
            let bright = host.overlay_commands().iter().any(|c| matches!(c, DrawCmd::Fill(col) if col.luminance() > 0.5));
            if bright && !prev {
                onsets += 1;
            }
            prev = bright;
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
        onsets
    };
    let tmp = tempfile::tempdir().unwrap();
    let mut host = strobe_host(tmp.path());
    assert_eq!(host.setting("strobe", "mode").unwrap().into_string().unwrap(), "Fixed rate");
    assert_eq!(host.setting("strobe", "rate").unwrap().as_float().unwrap(), 12.0);
    assert_eq!(host.setting("strobe", "display").unwrap().into_string().unwrap(), "Fullscreen");
    host.hotkey_key("F", CTRL);
    host.hotkey_key("U", CTRL);
    let n = count_onsets(&mut host);
    assert!((3..=7).contains(&n), "without opt-in: ~3 Hz, got {n} flashes in 2 s");
    host.key("Escape");

    host.set_flash_safety(true, 12.0);
    // Wall-clock timing: on a loaded CI runner a 2 s window can sample too few
    // frames to see every 12 Hz flash, so allow a couple of retries.
    let mut n = 0;
    for _ in 0..3 {
        host.hotkey_key("F", CTRL);
        host.hotkey_key("U", CTRL);
        n = count_onsets(&mut host);
        if n >= 12 {
            break;
        }
        host.key("Escape");
    }
    assert!(n >= 12, "with opt-in: much faster, got {n} flashes in 2 s");
    assert!(n <= 25, "never above 12 Hz, got {n}");
}

#[test]
fn hotkeys_need_permission_and_valid_chords() {
    let base = "id = \"h\"\nname = \"H\"\nversion = \"1\"\napi = 1\npermissions = [PERMS]\n[[hotkeys]]\nkeys = \"KEYS\"\naction = \"go\"\n";
    let m = |p: &str, k: &str| Manifest::parse(&base.replace("PERMS", p).replace("KEYS", k));
    assert!(m("\"hotkeys\"", "Ctrl+F,U").is_ok());
    assert!(m("", "Ctrl+F,U").unwrap_err().join("").contains("hotkeys"));
    assert!(m("\"hotkeys\"", "F").is_err());
    assert!(m("\"hotkeys\"", "Ctrl+Escape").is_err());
}

#[test]
fn install_bundled_upgrades_older_versions_with_backup() {
    let tmp = tempfile::tempdir().unwrap();
    install_bundled(tmp.path());
    let toml = tmp.path().join("strobe/plugin.toml");
    let src = std::fs::read_to_string(&toml).unwrap();
    let cur = src.lines().find(|l| l.starts_with("version = ")).unwrap();
    let old = src.replace(cur, "version = \"1.0.0\"");
    assert_ne!(src, old);
    std::fs::write(&toml, &old).unwrap();
    assert_eq!(install_bundled(tmp.path()), vec!["strobe".to_string()]);
    assert_eq!(std::fs::read_to_string(&toml).unwrap(), src);
    assert!(tmp.path().join(".strobe-backup-1.0.0/plugin.toml").is_file());
    let host = PluginHost::new(tmp.path());
    assert_eq!(host.plugins.len(), BUNDLED.len(), "backups are not loaded");
}

fn start_strobe_with(tmp: &Path, settings: &[(&str, serde_json::Value)]) -> PluginHost {
    let mut host = strobe_host(tmp);
    for (k, v) in settings {
        host.set_setting("strobe", k, v.clone());
    }
    host.reload("strobe");
    host.hotkey_key("F", CTRL);
    host.hotkey_key("U", CTRL);
    assert!(host.overlay.is_some());
    host
}

fn has_rings(cmds: &[DrawCmd]) -> bool {
    cmds.iter().filter(|c| matches!(c, DrawCmd::Rings { .. })).count() == 2
}

#[test]
fn strobe_rings_in_random_beat_mode_under_symbols() {
    let tmp = tempfile::tempdir().unwrap();
    let mut host = start_strobe_with(tmp.path(), &[("mode", "Random beat".into())]);
    let mut beat = AudioFrame { playing: true, ..Default::default() };
    beat.features.level = 0.5;
    beat.features.bass = 0.8;
    beat.features.quiet = false;
    let mut saw_rings = 0;
    let mut symbol_above_rings = false;
    let mut flashes_with_rings = 0;
    let mut lums = Vec::new();
    for i in 0..400 {
        beat.features.beat = i % 20 == 0; // 180 BPM
        host.frame(1.0 / 60.0, &beat);
        let cmds = host.overlay_commands();
        if has_rings(&cmds) {
            saw_rings += 1;
            let r = cmds.iter().rposition(|c| matches!(c, DrawCmd::Rings { .. })).unwrap();
            if let Some(p) = cmds.iter().position(|c| matches!(c, DrawCmd::Shape { .. })) {
                symbol_above_rings |= p > r;
                assert!(p > r, "symbols must be drawn over the rings");
            }
            if matches!(cmds[0], DrawCmd::Fill(c) if c.luminance() > 0.5) {
                flashes_with_rings += 1;
            }
        }
        lums.push(frame_luminance(&cmds, 16.0 / 9.0));
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(saw_rings > 300, "rings visible between flashes ({saw_rings})");
    assert!(flashes_with_rings > 0, "rings stay visible during flashes");
    let _ = symbol_above_rings;
    assert!(host.plugins[0].log.iter().all(|l| !l.starts_with("error")), "{:?}", host.plugins[0].log);

    // Black during breaks.
    beat.features.quiet = true;
    beat.features.beat = false;
    host.frame(1.0 / 60.0, &beat);
    assert_eq!(host.overlay_commands(), vec![DrawCmd::Fill(Rgba::BLACK)]);
}

#[test]
fn strobe_rings_settings() {
    // Rings are on by default in every mode and display, from the hotkey.
    for display in ["Fullscreen", "In window"] {
        for mode in ["Fixed rate", "Random beat"] {
            let tmp = tempfile::tempdir().unwrap();
            let mut host = start_strobe_with(tmp.path(), &[("mode", mode.into()), ("display", display.into())]);
            assert_eq!(host.overlay.as_ref().unwrap().fullscreen, display == "Fullscreen");
            host.frame(0.016, &AudioFrame::default());
            assert!(has_rings(&host.overlay_commands()), "{mode} / {display}");
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    let mut host = start_strobe_with(tmp.path(), &[("rings", false.into())]);
    host.frame(0.016, &AudioFrame::default());
    assert!(!has_rings(&host.overlay_commands()));
}

#[test]
fn strobe_rings_settings_migrate_from_1_2() {
    let mut st: HostState = serde_json::from_str(
        r#"{"settings":{"strobe":{"mode":"Fixed rate","rings":false,"rings_fixed":false}}}"#).unwrap();
    assert!(bwr_plugins::migrate_state(&mut st));
    assert!(!st.settings["strobe"].contains_key("rings"));
    assert!(!st.settings["strobe"].contains_key("rings_fixed"));
    // Random beat + rings off was a deliberate choice: kept.
    let mut st: HostState = serde_json::from_str(
        r#"{"settings":{"strobe":{"mode":"Random beat","rings":false,"rings_fixed":true}}}"#).unwrap();
    assert!(bwr_plugins::migrate_state(&mut st));
    assert_eq!(st.settings["strobe"]["rings"], serde_json::json!(false));
    // Already migrated: no-op.
    assert!(!bwr_plugins::migrate_state(&mut st));
    // Written to disk when the host starts.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("plugins-state.json"),
        r#"{"settings":{"strobe":{"rings_fixed":false}}}"#).unwrap();
    let _h = PluginHost::new(tmp.path());
    let saved = std::fs::read_to_string(tmp.path().join("plugins-state.json")).unwrap();
    assert!(!saved.contains("rings_fixed"), "{saved}");
}

#[test]
fn rings_api_validates() {
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(
        tmp.path(),
        "rg",
        "\"overlay\"",
        "fn ok(ctx) { let d = 0; }\nfn many(ctx) {}\n",
    );
    let dir = write_plugin(
        tmp.path(),
        "rg2",
        "\"overlay\"",
        "fn on_overlay(ctx, draw) { draw.rings(0.5, 0.5, 1, 0.001, [\"#fff\"]); }",
    );
    let r = check_plugin(&dir);
    assert!(r.to_string().contains("too many rings"), "{r}");
    let dir = write_plugin(tmp.path(), "rg3", "\"overlay\"", "fn on_overlay(ctx, draw) { draw.rings(0.5, 0.5, 0.9, 0.02, [\"#ffffff\", \"#00000000\"]); }");
    assert!(check_plugin(&dir).ok());
}

#[test]
fn shape_api_and_triangulation() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = write_plugin(tmp.path(), "sh1", "\"overlay\"",
        "fn on_overlay(ctx, draw) { draw.shape(\"M10 10 H90 V90 H10 Z\", 0.5, 0.5, 0.5, 0.03, \"#ff0000\", \"#00ff00\"); draw.shape(\"M10 10 L90 90\", 0.5, 0.5, 0.5, 0.03, \"#ff0000\", \"\"); }");
    let _ = dir;
    // Concave L-shape: 6 vertices -> 4 triangles covering its area.
    let l = vec![(0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0), (1.0, 2.0), (0.0, 2.0), (0.0, 0.0)];
    assert!(is_closed(&l));
    let tris = triangulate(&l);
    assert_eq!(tris.len(), 4);
    let area: f32 = tris.iter().map(|t| poly_area(&[l[t[0]], l[t[1]], l[t[2]]])).sum();
    assert!((area - poly_area(&l)).abs() < 1e-4, "{area}");
}

#[test]
fn strobe_symbols_are_bold_and_stay_up() {
    // The strobe uses the real clock, so this runs in real time (~5 s).
    let tmp = tempfile::tempdir().unwrap();
    let mut host = start_strobe_with(tmp.path(), &[("mode", "Random beat".into()), ("sym_time", "Random 1.5-3 s".into())]);
    let mut beat = AudioFrame { playing: true, ..Default::default() };
    beat.features.level = 0.5;
    beat.features.bass = 0.8;
    beat.features.quiet = false;
    let (mut changes, mut last, mut frames_with) = (0, String::new(), 0);
    let start = std::time::Instant::now();
    let mut next_beat = 0.0;
    while start.elapsed().as_secs_f64() < 5.0 {
        let t = start.elapsed().as_secs_f64();
        beat.features.beat = t >= next_beat;
        if beat.features.beat { next_beat += 0.25; }
        host.frame(1.0 / 60.0, &beat);
        let cmds = host.overlay_commands();
        let shapes: Vec<_> = cmds.iter().filter_map(|c| match c {
            DrawCmd::Shape { width, paths, .. } => Some((*width, paths.len())),
            _ => None,
        }).collect();
        std::thread::sleep(std::time::Duration::from_millis(10));
        if shapes.is_empty() { continue; }
        frames_with += 1;
        assert!(shapes.iter().any(|(w, _)| *w >= 0.02), "strokes scale with the screen height");
        let key = format!("{shapes:?}");
        if key != last { changes += 1; last = key; }
    }
    assert!(frames_with > 50, "{frames_with}");
    // Held 1.5-3 s each: at most 4 different symbols in 5 s.
    assert!(changes <= 4, "symbols change too often: {changes}");
}

/// Run the strobe in real time for `secs`, returning per-frame
/// (time, frame luminance, symbol key, veil alpha).
fn run_strobe(settings: &[(&str, serde_json::Value)], secs: f64) -> Vec<(f64, f32, String, u8)> {
    let tmp = tempfile::tempdir().unwrap();
    let mut host = start_strobe_with(tmp.path(), settings);
    let mut beat = AudioFrame { playing: true, ..Default::default() };
    beat.features.level = 0.5;
    beat.features.bass = 0.8;
    beat.features.quiet = false;
    let start = std::time::Instant::now();
    let mut next_beat = 0.0;
    let mut out = Vec::new();
    while start.elapsed().as_secs_f64() < secs {
        let t = start.elapsed().as_secs_f64();
        beat.features.beat = t >= next_beat;
        if beat.features.beat { next_beat += 0.25; }
        host.frame(1.0 / 60.0, &beat);
        let cmds = host.overlay_commands();
        let key = cmds.iter().filter_map(|c| match c {
            DrawCmd::Shape { paths, .. } => Some(paths.len().to_string()),
            DrawCmd::Text { text, size, .. } if *size > 40.0 => Some(text.clone()),
            _ => None,
        }).collect::<Vec<_>>().join("|");
        let veil = cmds.iter().filter_map(|c| match c {
            DrawCmd::Rect { x, y, w, h, color } if *x == 0.0 && *y == 0.0 && *w == 1.0 && *h == 1.0 && color.0[0] == 0 => Some(color.0[3]),
            _ => None,
        }).next().unwrap_or(0);
        out.push((t, frame_luminance(&cmds, 16.0 / 9.0), key, veil));
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
    out
}

#[test]
fn strobe_symbol_duration_default_half_second() {
    let frames = run_strobe(&[], 4.0);
    let mut changes = 0;
    let mut last = String::new();
    for (_, _, k, _) in &frames {
        if !k.is_empty() && *k != last { changes += 1; last = k.clone(); }
    }
    // 0.5 s per symbol at the 3 Hz cap -> a new one every ~0.67 s.
    assert!(changes >= 4, "symbols should change about every 0.5 s: {changes}");
}

#[test]
fn strobe_fade_to_black_cycle() {
    for mode in ["Fixed rate", "Random beat"] {
        let frames = run_strobe(&[("mode", mode.into()), ("fade", "1 s".into()), ("on_time", "5 s".into())], 8.0);
        // Before the on time ends: no veil.
        assert!(frames.iter().filter(|f| f.0 < 4.8).all(|f| f.3 == 0), "{mode}: no fade during the on time");
        // Smooth ramp: intermediate veil levels, small steps.
        let partial = frames.iter().filter(|f| f.3 > 20 && f.3 < 235).count();
        assert!(partial >= 20, "{mode}: fade is a ramp, not a cut ({partial} frames)");
        let mut prev = 0i32;
        let mut prev_t = 0.0;
        // (Frames the host limiter blacked out entirely carry no veil.)
        for f in frames.iter().filter(|f| f.3 > 0 || f.1 >= 0.05) {
            // A 1 s fade moves ~255 per second; allow for a stalled frame on a
            // loaded CI runner (wall-clock time) instead of a fixed step.
            let allowed = 40.0f64.max(255.0 * (f.0 - prev_t) as f64 * 1.5);
            assert!(((f.3 as i32 - prev).abs() as f64) <= allowed, "{mode}: veil jumps from {prev} to {} in {:.3} s", f.3, f.0 - prev_t);
            prev = f.3 as i32;
            prev_t = f.0;
        }
        // Held fully black for about 1 s, with nothing flashing.
        let black: Vec<_> = frames.iter().filter(|f| f.3 == 255).collect();
        let span = black.last().unwrap().0 - black.first().unwrap().0;
        assert!(span > 0.8 && span < 1.3, "{mode}: hold {span}");
        assert!(black.iter().all(|f| f.1 < 0.05), "{mode}: black during the hold");
        // Back in afterwards.
        assert!(frames.iter().any(|f| f.0 > black.last().unwrap().0 + 0.9 && f.3 == 0), "{mode}: fades back in");
    }
}

#[test]
fn strobe_fade_off_by_default_and_breaks_still_black() {
    let frames = run_strobe(&[], 1.5);
    assert!(frames.iter().all(|f| f.3 == 0));
    let tmp = tempfile::tempdir().unwrap();
    let mut host = start_strobe_with(tmp.path(), &[("fade", "3 s".into())]);
    let mut a = AudioFrame { playing: true, ..Default::default() };
    a.features.quiet = true;
    host.frame(1.0 / 60.0, &a);
    assert_eq!(host.overlay_commands(), vec![DrawCmd::Fill(Rgba::BLACK)]);
}

fn visions_host(tmp: &Path) -> PluginHost {
    install_bundled(tmp);
    let mut host = PluginHost::new(tmp);
    host.approve("visions");
    host
}

fn visual_of(cmds: &[DrawCmd]) -> Option<VisualParams> {
    cmds.iter().find_map(|c| match c {
        DrawCmd::Visual(v) => Some((**v).clone()),
        _ => None,
    })
}

#[test]
fn visions_hotkey_opens_scene_and_arrows_switch() {
    let tmp = tempfile::tempdir().unwrap();
    let mut host = visions_host(tmp.path());
    assert!(host.hotkeys().iter().any(|(id, h)| id == "visions" && h.keys == "Ctrl+M,D"));
    host.hotkey_key("M", CTRL);
    host.hotkey_key("D", CTRL);
    let s = host.overlay.clone().expect("open");
    assert!(s.fullscreen);
    assert_eq!(s.phase, OverlayPhase::Running);
    let mut a = AudioFrame { playing: true, ..Default::default() };
    a.features.level = 0.5;
    a.features.bass = 0.9;
    a.features.beat = true;
    host.frame(1.0 / 60.0, &a);
    let v = visual_of(&host.overlay_commands()).expect("draws a visual");
    assert_eq!(v.scene, "tunnel");
    assert!(v.pulse > 0.0, "beats become pulses");
    assert!(SCENES.contains(&v.scene.as_str()));
    host.key("ArrowRight");
    host.frame(1.0 / 60.0, &AudioFrame::default());
    let v = visual_of(&host.overlay_commands()).unwrap();
    assert_eq!(v.scene_b, "kaleido", "right arrow crossfades to the next scene");
    assert!(host.overlay.is_some());
    host.key("ArrowLeft");
    host.key("ArrowLeft");
    host.frame(1.0 / 60.0, &AudioFrame::default());
    assert_eq!(visual_of(&host.overlay_commands()).unwrap().scene_b, "mandala", "left wraps around");
    host.key("Escape");
    assert!(host.overlay.is_none());
}

#[test]
fn visions_page_previews_every_scene() {
    let tmp = tempfile::tempdir().unwrap();
    let mut host = visions_host(tmp.path());
    let w = host.page("visions", "visions");
    let dbg = format!("{w:?}");
    for s in SCENES {
        assert!(dbg.contains(&format!("scene: \"{s}\"")), "preview for {s}");
    }
}

#[test]
fn non_flashing_plugins_skip_the_warning() {
    let tmp = tempfile::tempdir().unwrap();
    for (id, flag, want) in [("calm", "flashing = false\n", OverlayPhase::Running), ("flashy", "", OverlayPhase::Warning)] {
        let dir = tmp.path().join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("plugin.toml"), format!("id = \"{id}\"\nname = \"{id}\"\nversion = \"1.0.0\"\napi = 1\npermissions = [\"overlay\"]\n{flag}")).unwrap();
        std::fs::write(dir.join("main.rhai"), "fn on_tick(ctx, dt) { if !ctx.overlay_active { ctx.open_overlay(false); } }\nfn on_overlay(ctx, draw) { draw.visual(#{ scene: \"plasma\" }); }").unwrap();
    }
    let mut host = PluginHost::new(tmp.path());
    for (id, want) in [("calm", OverlayPhase::Running), ("flashy", OverlayPhase::Warning)] {
        host.approve(id);
        host.frame(0.016, &AudioFrame::default());
        let _ = host.take_requests();
        assert_eq!(host.overlay.as_ref().map(|s| s.phase), Some(want), "{id}");
        host.end_overlay();
        host.set_enabled(id, false);
    }
}

#[test]
fn visual_api_validates() {
    let tmp = tempfile::tempdir().unwrap();
    for (n, body, err) in [
        ("v1", "draw.visual(#{ scene: \"nope\" });", "unknown scene"),
        ("v2", "draw.visual(#{ scene: \"plasma\", wobble: 1 });", "unknown key"),
    ] {
        write_plugin(tmp.path(), n, "\"overlay\"", &format!("fn on_overlay(ctx, draw) {{ {body} }}"));
        let r = check_plugin(&tmp.path().join(n));
        let msg = format!("{r:?}");
        assert!(msg.contains(err), "{n}: {msg}");
    }
}

#[test]
fn card_tints_are_stable_and_honour_accent() {
    assert_eq!(card_tint("visions", ""), card_tint("visions", ""));
    assert_ne!(card_tint("strobe", ""), card_tint("visions", ""));
    assert_eq!(card_tint("anything", "#123456"), Rgba([0x12, 0x34, 0x56, 255]));
    assert_eq!(card_tint("x", "not a colour"), card_tint("x", ""));
    // Every bundled plugin declares a valid accent.
    for (folder, files) in BUNDLED {
        let m = Manifest::parse(files.iter().find(|f| f.0 == "plugin.toml").unwrap().1).unwrap();
        assert!(Rgba::parse(&m.accent).is_some(), "{folder}");
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = write_plugin(tmp.path(), "acc", "", "fn on_load(ctx) {}");
    let toml = std::fs::read_to_string(dir.join("plugin.toml")).unwrap() + "accent = \"purple-ish\"\n";
    std::fs::write(dir.join("plugin.toml"), toml).unwrap();
    let r = check_plugin(&dir);
    assert!(r.ok() && r.warnings.iter().any(|w| w.contains("accent")), "{r}");
}
