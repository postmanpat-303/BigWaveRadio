//! Plugins shipped inside the app and installed on first run.

use std::path::Path;

/// (folder, [(file name, contents)])
pub const BUNDLED: &[(&str, &[(&str, &str)])] = &[(
    "strobe",
    &[
        ("plugin.toml", include_str!("../../../plugins/strobe/plugin.toml")),
        ("main.rhai", include_str!("../../../plugins/strobe/main.rhai")),
    ],
)];

fn version_of(toml_src: &str) -> Vec<u64> {
    toml::from_str::<toml::Value>(toml_src)
        .ok()
        .and_then(|v| v.get("version").and_then(|v| v.as_str()).map(str::to_string))
        .unwrap_or_default()
        .split('.')
        .map(|p| p.trim().parse().unwrap_or(0))
        .collect()
}

/// Copy bundled plugins into `plugins_dir`. A missing folder is created; an
/// installed copy with an *older* `version` is upgraded, after moving it to
/// `.<folder>-backup-<version>` (dot folders are not loaded), so edits are
/// never lost. Same or newer versions are left alone. Also drops the
/// authoring guide next to them. Returns the folders installed/upgraded.
pub fn install_bundled(plugins_dir: &Path) -> Vec<String> {
    let mut installed = Vec::new();
    let _ = std::fs::create_dir_all(plugins_dir);
    let _ = std::fs::write(plugins_dir.join("PLUGIN_GUIDE.md"), include_str!("../../../docs/PLUGIN_GUIDE.md"));
    for (folder, files) in BUNDLED {
        let dir = plugins_dir.join(folder);
        if dir.exists() {
            let installed = std::fs::read_to_string(dir.join("plugin.toml")).unwrap_or_default();
            let bundled = files.iter().find(|(n, _)| *n == "plugin.toml").map(|(_, b)| *b).unwrap_or("");
            let (old, new) = (version_of(&installed), version_of(bundled));
            if old.is_empty() || old >= new {
                continue;
            }
            let tag = old.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(".");
            let backup = plugins_dir.join(format!(".{folder}-backup-{tag}"));
            let _ = std::fs::remove_dir_all(&backup);
            if std::fs::rename(&dir, &backup).is_err() {
                continue;
            }
        }
        if std::fs::create_dir_all(&dir).is_err() {
            continue;
        }
        let ok = files.iter().all(|(name, body)| std::fs::write(dir.join(name), body).is_ok());
        if ok {
            installed.push(folder.to_string());
        }
    }
    installed
}
