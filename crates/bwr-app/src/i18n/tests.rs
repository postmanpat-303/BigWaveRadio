//! Catalog checks: every translation covers English exactly, placeholders
//! match, and every English key is used by the code (no orphans).

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use super::*;

fn catalog(lang: Lang) -> HashMap<String, &'static str> {
    parse_catalog(lang.source()).unwrap_or_else(|e| panic!("{}.toml does not parse: {e}", lang.code()))
}

/// `{name}` placeholders in a text.
fn placeholders(s: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        rest = &rest[i + 1..];
        if let Some(j) = rest.find('}') {
            out.insert(rest[..j].to_string());
            rest = &rest[j + 1..];
        }
    }
    out
}

/// Every string literal in `src/` (keys are only ever written as literals).
fn source_literals() -> BTreeSet<String> {
    fn walk(dir: &Path, out: &mut BTreeSet<String>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                if !p.ends_with("i18n") {
                    walk(&p, out);
                }
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&p).unwrap();
                let mut parts = text.split('"');
                parts.next();
                // Odd segments are inside quotes; good enough for key-shaped literals.
                for (i, s) in parts.enumerate() {
                    if i % 2 == 0 && !s.is_empty() && !s.contains(char::is_whitespace) {
                        out.insert(s.to_string());
                    }
                }
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    out
}

/// Keys the code asks for: `t!("k")` -> k, `tn!("k", n)` -> k.one + k.other.
fn used_keys() -> BTreeSet<String> {
    fn walk(dir: &Path, out: &mut BTreeSet<String>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            // Skip the i18n module itself (docs and these tests mention keys).
            if p.is_dir() {
                if !p.ends_with("i18n") {
                    walk(&p, out);
                }
            } else if p.extension().is_some_and(|x| x == "rs") && !p.ends_with("i18n.rs") {
                let text = std::fs::read_to_string(&p).unwrap();
                for (mac, plural) in [("t!(\"", false), ("t_raw!(\"", false), ("tn!(\"", true)] {
                    for (i, _) in text.match_indices(mac) {
                        // Whole macro names only (`format!(` also ends in "t!(").
                        if text[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            continue;
                        }
                        let key = text[i + mac.len()..].split('"').next().unwrap();
                        if plural {
                            out.insert(format!("{key}.one"));
                            out.insert(format!("{key}.other"));
                        } else {
                            out.insert(key.to_string());
                        }
                    }
                }
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    out
}

#[test]
fn every_used_key_exists_in_english() {
    let en = catalog(Lang::En);
    let missing: Vec<_> = used_keys().into_iter().filter(|k| !en.contains_key(k)).collect();
    assert!(missing.is_empty(), "keys used in code but missing from en.toml: {missing:?}");
}

#[test]
fn no_orphan_english_keys() {
    let literals = source_literals();
    let used = used_keys();
    let orphans: Vec<_> = catalog(Lang::En)
        .into_keys()
        .filter(|k| {
            if used.contains(k) || literals.contains(k) {
                return false;
            }
            // Plural forms are used through their base key.
            let base = k.strip_suffix(".one").or_else(|| k.strip_suffix(".other"));
            !base.is_some_and(|b| used.contains(&format!("{b}.one")))
        })
        .collect();
    assert!(orphans.is_empty(), "en.toml keys no code uses: {orphans:?}");
}

#[test]
fn translations_cover_english_exactly() {
    let en = catalog(Lang::En);
    for lang in Lang::ALL.into_iter().filter(|l| *l != Lang::En) {
        let cat = catalog(lang);
        let missing: BTreeSet<_> = en.keys().filter(|k| !cat.contains_key(*k)).collect();
        let extra: BTreeSet<_> = cat.keys().filter(|k| !en.contains_key(*k)).collect();
        let coverage = 100.0 * (en.len() - missing.len()) as f64 / en.len() as f64;
        println!("{}: {}/{} keys ({coverage:.1}%)", lang.code(), en.len() - missing.len(), en.len());
        assert!(missing.is_empty(), "{}.toml is missing {missing:?}", lang.code());
        assert!(extra.is_empty(), "{}.toml has keys English doesn't: {extra:?}", lang.code());
        for (k, v) in &cat {
            assert_eq!(
                placeholders(v),
                placeholders(en[k]),
                "{}.toml `{k}`: placeholders differ from English",
                lang.code()
            );
            assert!(!v.trim().is_empty(), "{}.toml `{k}` is empty", lang.code());
        }
    }
}

#[test]
fn locale_mapping() {
    assert_eq!(Lang::from_locale("pt-BR"), Lang::PtBr);
    assert_eq!(Lang::from_locale("pt_PT.UTF-8"), Lang::PtBr);
    assert_eq!(Lang::from_locale("it-IT"), Lang::It);
    assert_eq!(Lang::from_locale("it_CH"), Lang::It);
    assert_eq!(Lang::from_locale("es-MX"), Lang::Es);
    assert_eq!(Lang::from_locale("es-ES"), Lang::Es);
    assert_eq!(Lang::from_locale("zh-CN"), Lang::ZhCn);
    assert_eq!(Lang::from_locale("zh-Hant-TW"), Lang::ZhCn);
    assert_eq!(Lang::from_locale("hi-IN"), Lang::Hi);
    assert_eq!(Lang::from_locale("ar-EG"), Lang::Ar);
    assert_eq!(Lang::from_locale("ar_SA"), Lang::Ar);
    assert_eq!(Lang::from_locale("en-IE"), Lang::En);
    assert_eq!(Lang::from_locale("de-DE"), Lang::En);
    assert_eq!(Lang::from_locale(""), Lang::En);
    assert_eq!(Choice::parse("system"), Choice::System);
    assert_eq!(Choice::parse("pt-br"), Choice::Fixed(Lang::PtBr));
    assert_eq!(Choice::parse("zh-cn"), Choice::Fixed(Lang::ZhCn));
    assert_eq!(Choice::parse("klingon"), Choice::System);
    for l in Lang::ALL {
        assert_eq!(Choice::parse(Choice::Fixed(l).code()), Choice::Fixed(l));
    }
}

#[test]
fn lookup_falls_back_and_fills() {
    assert_eq!(lookup(Lang::It, "nav.settings"), "Impostazioni");
    assert_eq!(lookup(Lang::PtBr, "nav.settings"), "Configurações");
    assert_eq!(lookup(Lang::It, "no.such.key"), "no.such.key");
    assert_eq!(fill("{a} and {b}", &[("a", &1), ("b", &"x")]), "1 and x");
    assert!(Lang::PtBr.is_one(0) && Lang::PtBr.is_one(1) && !Lang::PtBr.is_one(2));
    assert!(!Lang::It.is_one(0) && Lang::It.is_one(1));
}

/// Arabic UI text is handed out in display form (joined presentation forms,
/// visual order); other languages are untouched.
#[test]
fn arabic_catalog_is_converted_for_display() {
    let raw = lookup(Lang::Ar, "cardsui.now_playing");
    assert!(crate::rtl::has_rtl(raw));
    let shown = visual_catalog(Lang::Ar)["cardsui.now_playing"];
    assert_eq!(shown, crate::rtl::visual_ui(raw, true));
    assert!(shown.chars().any(|c| ('\u{FE70}'..='\u{FEFF}').contains(&c)), "{shown}");
    // Every Arabic entry converts, and keys are the same set.
    assert_eq!(visual_catalog(Lang::Ar).len(), catalogs()[Lang::Ar.index()].len());
    // Filled templates: arguments join the bidi ordering of the sentence.
    assert_eq!(show(Lang::En, "♪ Hello".to_string()), "♪ Hello");
    let song = show(Lang::En, fill(lookup(Lang::En, "cardsui.now_line"), &[("song", &"سلام")]));
    assert_eq!(song, format!("{} ♪", crate::rtl::visual("سلام")));
}
