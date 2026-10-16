//! `bwr-app --check-plugin <dir>`: validate a plugin without the GUI.

use std::fmt;
use std::path::Path;

use crate::host::{PluginHost, PluginStatus, HOOKS};
use crate::manifest::Manifest;

#[derive(Debug, Default, Clone)]
pub struct CheckReport {
    pub id: String,
    pub name: String,
    pub hooks: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl CheckReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

impl fmt::Display for CheckReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.id.is_empty() {
            writeln!(f, "plugin: {} ({})", self.name, self.id)?;
        }
        if !self.hooks.is_empty() {
            writeln!(f, "hooks: {}", self.hooks.join(", "))?;
        }
        for w in &self.warnings {
            writeln!(f, "warning: {w}")?;
        }
        for e in &self.errors {
            writeln!(f, "error: {e}")?;
        }
        if self.ok() {
            writeln!(f, "OK")
        } else {
            writeln!(f, "FAILED ({} error(s))", self.errors.len())
        }
    }
}

/// Validate the manifest, compile the script, check hook signatures and
/// permissions, then run every hook once against fake data.
pub fn check_plugin(dir: &Path) -> CheckReport {
    let mut r = CheckReport::default();
    let manifest_path = dir.join("plugin.toml");
    let src = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            r.errors.push(format!("cannot read {}: {e}", manifest_path.display()));
            return r;
        }
    };
    let m = match Manifest::parse(&src) {
        Ok(m) => m,
        Err(errs) => {
            r.errors.extend(errs);
            return r;
        }
    };
    r.id = m.id.clone();
    r.name = m.name.clone();
    if let Some(folder) = dir.file_name().map(|n| n.to_string_lossy().into_owned()) {
        if folder != m.id && folder != "." {
            r.warnings.push(format!("folder name \"{folder}\" differs from id \"{}\" (zip import uses the id)", m.id));
        }
    }

    let mut host = PluginHost::detached(dir);
    let Some(p) = host.plugins.first() else {
        r.errors.push("plugin did not load".into());
        return r;
    };
    if let PluginStatus::Error(e) = &p.status {
        r.errors.push(e.clone());
        return r;
    }
    let perms = m.permission_set();
    for (name, arity) in p_hooks(&host) {
        match HOOKS.iter().find(|(n, _, _)| *n == name) {
            Some((_, want, perm)) => {
                if arity != *want {
                    r.errors.push(format!("{name} takes {want} parameter(s), found {arity}"));
                }
                if let Some(perm) = perm {
                    if !perms.contains(perm) {
                        r.warnings.push(format!("{name} will never be called without the \"{}\" permission", perm.name()));
                    }
                }
                r.hooks.push(name);
            }
            None if name.starts_with("on_") => {
                r.warnings.push(format!("{name} is not a known hook (known: {})", HOOKS.iter().map(|h| h.0).collect::<Vec<_>>().join(", ")));
            }
            None => {}
        }
    }
    if !m.pages.is_empty() && !r.hooks.iter().any(|h| h == "on_page") {
        r.warnings.push("[[pages]] declared but there is no on_page(ctx, page_id, ui) hook".into());
    }
    if !m.accent.is_empty() && crate::Rgba::parse(m.accent.trim()).is_none() {
        r.warnings.push(format!("accent \"{}\" is not a colour (use \"#rrggbb\")", m.accent));
    }
    if r.errors.is_empty() {
        r.errors.extend(host.smoke_test(&m.id).into_iter().map(|e| format!("runtime: {e}")));
    }
    r
}

fn p_hooks(host: &PluginHost) -> Vec<(String, usize)> {
    host.plugins.first().map(|p| p.hook_list()).unwrap_or_default()
}
