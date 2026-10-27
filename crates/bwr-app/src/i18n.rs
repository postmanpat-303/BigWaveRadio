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
    Es,
    ZhCn,
    Hi,
    Ar,
    PtBr,
    It,
}

impl Lang {
    /// Every language, in the order shown in Settings: the five most-used
    /// worldwide first, then the earlier additions.
    pub const ALL: [Lang; 7] = [Lang::En, Lang::Es, Lang::ZhCn, Lang::Hi, Lang::Ar, Lang::PtBr, Lang::It];

    /// BCP 47 code: stored in ui.json and passed to plugins as `ctx.lang`.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Es => "es",
            Lang::ZhCn => "zh-CN",
            Lang::Hi => "hi",
            Lang::Ar => "ar",
            Lang::PtBr => "pt-BR",
            Lang::It => "it",
        }
    }

    /// The language's name in that language (what the selector shows).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Es => "Español",
            Lang::ZhCn => "中文（简体）",
            Lang::Hi => "हिन्दी",
            Lang::Ar => "العربية",
            Lang::PtBr => "Português (Brasil)",
            Lang::It => "Italiano",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code().eq_ignore_ascii_case(code.trim()))
    }

    /// Map an OS locale ("pt-BR", "es-MX", "zh-Hans", "hi-IN", "ar_EG",
    /// "it-IT", "en-IE", ...) to a supported language.
    pub fn from_locale(locale: &str) -> Lang {
        let primary = locale
            .trim()
            .split(|c: char| c == '-' || c == '_' || c == '.' || c == '@')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match primary.as_str() {
            "es" => Lang::Es,
            "zh" => Lang::ZhCn,
            "hi" => Lang::Hi,
            "ar" => Lang::Ar,
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
            Lang::Es => include_str!("../locales/es.toml"),
            Lang::ZhCn => include_str!("../locales/zh-CN.toml"),
            Lang::Hi => include_str!("../locales/hi.toml"),
            Lang::Ar => include_str!("../locales/ar.toml"),
            Lang::PtBr => include_str!("../locales/pt-BR.toml"),
            Lang::It => include_str!("../locales/it.toml"),
        }
    }

    /// CLDR cardinal rule, reduced to the integer cases the UI needs.
    fn is_one(self, n: i64) -> bool {
        match self {
            // Portuguese (Brazil): 0 and 1 are both singular ("0 estação").
            Lang::PtBr => n == 0 || n == 1,
            // Chinese: no plural distinction — the catalog's .one text is the
            // same as .other, so always take .other.
            Lang::ZhCn => false,
            Lang::En | Lang::Es | Lang::Hi | Lang::Ar | Lang::It => n == 1,
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
    let idx = CURRENT.load(Ordering::Relaxed) as usize;
    Lang::ALL.get(idx).copied().unwrap_or(Lang::En)
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

fn catalogs() -> &'static [Catalog; 7] {
    static CATS: OnceLock<[Catalog; 7]> = OnceLock::new();
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

/// Font family of the big station-name readout (bundled Inter Light,
/// SIL OFL 1.1 — see assets/fonts/Inter-OFL.txt). It falls back to the UI
/// fonts, so names in other scripts still render.
pub const READOUT_FAMILY: &str = "readout";

const INTER_LIGHT: &[u8] = include_bytes!("../../../assets/fonts/Inter-Light.ttf");

/// Candidate system fonts per script: Windows first, then common Linux and
/// macOS locations. `(path, face index in a collection)`.
fn script_font_candidates(lang: Lang) -> &'static [(&'static str, u32)] {
    match lang {
        Lang::ZhCn => &[
            (r"C:\Windows\Fonts\msyh.ttc", 0),
            ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 2),
            ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 2),
            ("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", 2),
            ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
            ("/System/Library/Fonts/PingFang.ttc", 0),
        ],
        Lang::Hi => &[
            (r"C:\Windows\Fonts\Nirmala.ttc", 0),
            ("/usr/share/fonts/truetype/noto/NotoSansDevanagari-Regular.ttf", 0),
            ("/usr/share/fonts/noto/NotoSansDevanagari-Regular.ttf", 0),
            ("/usr/share/fonts/truetype/sand-box/google/Noto Sans Devanagari/NotoSansDevanagari-VariableFont_wdth,wght.ttf", 0),
            ("/usr/share/fonts/truetype/lohit-devanagari/Lohit-Devanagari.ttf", 0),
            ("/System/Library/Fonts/Supplemental/DevanagariMT.ttc", 0),
        ],
        Lang::Ar => &[
            (r"C:\Windows\Fonts\arial.ttf", 0),
            ("/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf", 0),
            ("/usr/share/fonts/noto/NotoSansArabic-Regular.ttf", 0),
            ("/usr/share/fonts/truetype/sand-box/google/Noto Sans Arabic/NotoSansArabic-VariableFont_wdth,wght.ttf", 0),
            ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 0),
            ("/System/Library/Fonts/Supplemental/Arial.ttf", 0),
        ],
        _ => &[],
    }
}

static FONT_MASK: AtomicU8 = AtomicU8::new(0);
static SCRIPT_FONTS: std::sync::Mutex<Vec<(String, std::sync::Arc<egui::FontData>)>> =
    std::sync::Mutex::new(Vec::new());

/// Install the app fonts: egui's defaults, the bundled readout face and —
/// because egui's bundled fonts cover Latin, Greek and Cyrillic only — a
/// system font for Chinese, Devanagari or Arabic once a language needing
/// it is chosen. Rebuilds only when that set grows (a cheap flag check per
/// frame otherwise). Missing files are skipped gracefully.
pub fn ensure_fonts(ctx: &egui::Context) {
    let lang = current();
    let script_bit = match lang {
        Lang::ZhCn | Lang::Hi | Lang::Ar => 1u8 << lang.index(),
        _ => 0,
    };
    let need = 0x80 | script_bit;
    let have = FONT_MASK.load(Ordering::Relaxed);
    if have & need == need {
        return;
    }
    FONT_MASK.fetch_or(need, Ordering::Relaxed);
    let mut loaded = SCRIPT_FONTS.lock().unwrap_or_else(|e| e.into_inner());
    if script_bit != 0 {
        let found = script_font_candidates(lang)
            .iter()
            .find_map(|(path, index)| std::fs::read(path).ok().map(|b| (b, *index)));
        if let Some((bytes, index)) = found {
            loaded.push((
                format!("bwr-script-{}", lang.code()),
                std::sync::Arc::new(egui::FontData {
                    font: std::borrow::Cow::Owned(bytes),
                    index,
                    tweak: Default::default(),
                }),
            ));
        }
    }
    let mut fonts = egui::FontDefinitions::default();
    for (name, data) in loaded.iter() {
        fonts.font_data.insert(name.clone(), data.clone());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(name.clone());
        }
    }
    fonts.font_data.insert(
        "bwr-inter-light".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(INTER_LIGHT)),
    );
    let mut readout = vec!["bwr-inter-light".to_owned()];
    readout.extend(fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(egui::FontFamily::Name(READOUT_FAMILY.into()), readout);
    ctx.set_fonts(fonts);
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
