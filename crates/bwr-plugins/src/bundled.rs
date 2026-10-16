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

/// Copy bundled plugins into `plugins_dir` unless a folder of that name
/// already exists (user edits are never overwritten). Also drops the
/// authoring guide next to them. Returns the folders installed.
pub fn install_bundled(plugins_dir: &Path) -> Vec<String> {
    let mut installed = Vec::new();
    let _ = std::fs::create_dir_all(plugins_dir);
    let _ = std::fs::write(plugins_dir.join("PLUGIN_GUIDE.md"), include_str!("../../../docs/PLUGIN_GUIDE.md"));
    for (folder, files) in BUNDLED {
        let dir = plugins_dir.join(folder);
        if dir.exists() {
            continue;
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
