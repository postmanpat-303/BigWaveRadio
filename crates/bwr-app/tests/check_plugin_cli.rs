//! `bwr-app --check-plugin <dir>` runs without opening a window.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bwr-app"))
}

#[test]
fn bundled_strobe_passes() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/strobe");
    let out = bin().arg("--check-plugin").arg(&dir).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("plugin: Beat Strobe (strobe)"), "{stdout}");
    assert!(stdout.trim_end().ends_with("OK"), "{stdout}");
}

#[test]
fn broken_plugin_fails_with_line_numbers() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("plugin.toml"), "id = \"x\"\nname = \"X\"\nversion = \"1\"\napi = 1\n").unwrap();
    std::fs::write(tmp.path().join("main.rhai"), "fn on_load(ctx) {\n  let = 3;\n}\n").unwrap();
    let out = bin().arg("--check-plugin").arg(tmp.path()).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("line 2"), "{stdout}");
    assert!(stdout.contains("FAILED"), "{stdout}");
}

#[test]
fn missing_argument_is_a_usage_error() {
    let out = bin().arg("--check-plugin").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}
