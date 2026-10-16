use std::path::{Path, PathBuf};

use bwr_plugins::*;

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
    assert_eq!(install_bundled(tmp.path()), vec!["strobe".to_string()]);
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
