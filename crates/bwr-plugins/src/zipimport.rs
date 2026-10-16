//! `.zip` import: unpack a plugin archive into the plugins folder safely.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::check::check_plugin;
use crate::manifest::Manifest;

const MAX_FILES: usize = 200;
const MAX_TOTAL: u64 = 20 * 1024 * 1024;

/// Import `zip_path` into `plugins_dir`. The archive must contain
/// `plugin.toml` at its root or inside a single top-level folder. The plugin
/// is installed as `plugins_dir/<id>` (replacing an older copy) only if it
/// passes the same checks as `--check-plugin`. Returns the plugin id.
pub fn import_zip(zip_path: &Path, plugins_dir: &Path) -> Result<String, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("cannot open {}: {e}", zip_path.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("not a valid .zip: {e}"))?;
    if zip.len() > MAX_FILES {
        return Err(format!("archive has {} entries (max {MAX_FILES})", zip.len()));
    }

    // Locate plugin.toml to find the root prefix.
    let mut prefix: Option<PathBuf> = None;
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(p) = f.enclosed_name() else { continue };
        if p.file_name().is_some_and(|n| n == "plugin.toml") && p.components().count() <= 2 {
            let parent = p.parent().map(Path::to_path_buf).unwrap_or_default();
            if prefix.as_ref().is_none_or(|old| parent.components().count() < old.components().count()) {
                prefix = Some(parent);
            }
        }
    }
    let prefix = prefix.ok_or("archive has no plugin.toml at its root or in a single top folder")?;

    std::fs::create_dir_all(plugins_dir).map_err(|e| e.to_string())?;
    let staging = plugins_dir.join(format!(".import-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut total = 0u64;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
            let name = f.enclosed_name().ok_or_else(|| format!("unsafe path in archive: {}", f.name()))?;
            let Ok(rel) = name.strip_prefix(&prefix) else { continue };
            if rel.as_os_str().is_empty() {
                continue;
            }
            let out = staging.join(rel);
            if f.is_dir() {
                std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
                continue;
            }
            total += f.size();
            if total > MAX_TOTAL {
                return Err("archive is too large (max 20 MB unpacked)".to_string());
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut buf = Vec::new();
            (&mut f).take(MAX_TOTAL + 1).read_to_end(&mut buf).map_err(|e| e.to_string())?;
            std::fs::write(&out, buf).map_err(|e| e.to_string())?;
        }
        let src = std::fs::read_to_string(staging.join("plugin.toml")).map_err(|e| e.to_string())?;
        let m = Manifest::parse(&src).map_err(|e| e.join("\n"))?;
        let report = check_plugin(&staging);
        if !report.ok() {
            return Err(format!("plugin failed checks:\n{report}"));
        }
        let dest = plugins_dir.join(&m.id);
        if dest.exists() {
            std::fs::remove_dir_all(&dest).map_err(|e| format!("cannot replace existing {}: {e}", dest.display()))?;
        }
        std::fs::rename(&staging, &dest).map_err(|e| e.to_string())?;
        Ok(m.id)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}
