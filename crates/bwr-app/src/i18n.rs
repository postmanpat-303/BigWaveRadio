//! User-interface translations.
//!
//! Catalogs are TOML files in `crates/bwr-app/locales/` (`en.toml` is the
//! source and the fallback), embedded into the binary at compile time. Keys
//! are `section.name` (TOML tables are flattened with dots). Use the
//! [`t!`](crate::t) macro:
//!
//! ```ignore
//! ui.heading(t!("discover.title"));                    // &'static str
//! ui.label(t!("browse.map_tip", name = n, count = c)); // String, fills {name} / {count}
//! ui.label(tn!("moods.talk_hidden", hidden));          // plural: picks .one / .other, fills {n}
//! ```
//!
//! A key missing from a translation falls back to English; a key missing
//! from English shows the key itself (and fails `cargo test`). The current
//! language is process-wide, so switching in Settings applies on the next
//! frame without a restart. See docs/TRANSLATING.md.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// A supported UI language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    PtBr,
    It,
}

impl Lang {
    /// Every language, in the order shown in Settings.
    pub const ALL: [Lang; 3] = [Lang::En, Lang::PtBr, Lang::It];

    /// BCP 47 code: stored in ui.json and passed to plugins as `ctx.lang`.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::PtBr => "pt-BR",
            Lang::It => "it",
        }
    }

    /// The language's name in that language (what the selector shows).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::PtBr => "Português (Brasil)",
            Lang::It => "Italiano",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code().eq_ignore_ascii_case(code.trim()))
    }

    /// Map an OS locale ("pt-BR", "pt_PT.UTF-8", "it-IT", "en-IE", ...) to a
    /// supported language: pt-* -> pt-BR, it-* -> it, anything else -> en.
    pub fn from_locale(locale: &str) -> Lang {
        let primary = locale
            .trim()
            .split(|c: char| c == '-' || c == '_' || c == '.' || c == '@')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match primary.as_str() {
            "pt" => Lang::PtBr,
            "it" => Lang::It,
            _ => Lang::En,
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn source(self) -> &'static str {
        match self {
            Lang::En => include_str!("../locales/en.toml"),
            Lang::PtBr => include_str!("../locales/pt-BR.toml"),
            Lang::It => include_str!("../locales/it.toml"),
        }
    }

    /// CLDR cardinal rule, reduced to the integer cases the UI needs.
    fn is_one(self, n: i64) -> bool {
        match self {
            // Portuguese (Brazil): 0 and 1 are both singular ("0 estação").
            Lang::PtBr => n == 0 || n == 1,
            Lang::En | Lang::It => n == 1,
        }
    }
}

/// The user's choice in Settings: follow the OS, or a fixed language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    System,
    Fixed(Lang),
}

impl Choice {
    /// Parse the `language` value of ui.json ("system", "en", "pt-BR", "it").
    pub fn parse(s: &str) -> Choice {
        Lang::from_code(s).map(Choice::Fixed).unwrap_or(Choice::System)
    }

    pub fn code(self) -> &'static str {
        match self {
            Choice::System => "system",
            Choice::Fixed(l) => l.code(),
        }
    }

    /// The language this choice resolves to right now.
    pub fn resolve(self) -> Lang {
        match self {
            Choice::Fixed(l) => l,
            Choice::System => system_lang(),
        }
    }
}

/// The OS UI language, mapped to a supported one (`BWR_LANG` overrides it,
/// for testing and screenshots).
pub fn system_lang() -> Lang {
    if let Some(l) = std::env::var("BWR_LANG").ok().and_then(|v| Lang::from_code(&v)) {
        return l;
    }
    sys_locale::get_locale().map(|l| Lang::from_locale(&l)).unwrap_or(Lang::En)
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Switch the UI language (takes effect on the next frame).
pub fn set(lang: Lang) {
    CURRENT.store(lang.index() as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::PtBr,
        2 => Lang::It,
        _ => Lang::En,
    }
}

type Catalog = HashMap<String, &'static str>;

/// Flatten a TOML document into `a.b.c -> text`. Strings are leaked once,
/// so lookups hand out `&'static str` (the catalogs live for the process).
pub fn parse_catalog(src: &str) -> Result<Catalog, String> {
    fn walk(prefix: &str, table: &toml::Table, out: &mut Catalog) -> Result<(), String> {
        for (k, v) in table {
            let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
            match v {
                toml::Value::String(s) => {
                    out.insert(key, Box::leak(s.clone().into_boxed_str()));
                }
                toml::Value::Table(t) => walk(&key, t, out)?,
                _ => return Err(format!("{key}: expected a string")),
            }
        }
        Ok(())
    }
    let table: toml::Table = src.parse().map_err(|e| format!("{e}"))?;
    let mut out = Catalog::new();
    walk("", &table, &mut out)?;
    Ok(out)
}

fn catalogs() -> &'static [Catalog; 3] {
    static CATS: OnceLock<[Catalog; 3]> = OnceLock::new();
    CATS.get_or_init(|| {
        Lang::ALL.map(|l| {
            parse_catalog(l.source()).unwrap_or_else(|e| {
                // A broken translation must never take the app down:
                // fall back to English for the whole language.
                eprintln!("[i18n] {}.toml: {e}", l.code());
                Catalog::new()
            })
        })
    })
}

/// Look up `key` in `lang`, falling back to English, then to the key itself.
pub fn lookup(lang: Lang, key: &'static str) -> &'static str {
    let cats = catalogs();
    cats[lang.index()]
        .get(key)
        .or_else(|| cats[Lang::En.index()].get(key))
        .copied()
        .unwrap_or(key)
}

/// `t!("key")`: the current language's text for `key`.
pub fn tr(key: &'static str) -> &'static str {
    lookup(current(), key)
}

/// Replace `{name}` placeholders.
pub fn fill(template: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut out = template.to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

/// `t!("key", a = x, ...)`.
pub fn tr_args(key: &'static str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    fill(tr(key), args)
}

/// `tn!("key", n)`: `key.one` or `key.other` for `n`, with `{n}` filled.
pub fn tr_plural(one: &'static str, other: &'static str, n: i64, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let lang = current();
    let key = if lang.is_one(n) { one } else { other };
    let mut all: Vec<(&str, &dyn std::fmt::Display)> = vec![("n", &n)];
    all.extend_from_slice(args);
    fill(lookup(lang, key), &all)
}

/// Translate a text by key; with `name = value` pairs it fills `{name}`.
macro_rules! t {
    ($key:literal) => {
        $crate::i18n::tr($key)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::tr_args($key, &[$((stringify!($name), &$value as &dyn std::fmt::Display)),+])
    };
}

/// Plural form of `key` (`key.one` / `key.other` in the catalog) for count
/// `n`, which fills `{n}`; extra `name = value` pairs work like `t!`.
macro_rules! tn {
    ($key:literal, $n:expr) => {
        $crate::i18n::tr_plural(concat!($key, ".one"), concat!($key, ".other"), ($n) as i64, &[])
    };
    ($key:literal, $n:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::tr_plural(
            concat!($key, ".one"),
            concat!($key, ".other"),
            ($n) as i64,
            &[$((stringify!($name), &$value as &dyn std::fmt::Display)),+],
        )
    };
}

#[cfg(test)]
mod tests;
