//! Right-to-left text for egui.
//!
//! egui lays text out strictly left to right, one glyph per char: it has no
//! bidi algorithm and no Arabic shaping. So before a string that contains
//! RTL characters reaches a label we
//!
//! 1. reshape Arabic letters into their joined presentation forms
//!    (isolated / initial / medial / final, lam-alef ligatures) with
//!    `ar-reshaper`, and
//! 2. reorder the result into *visual* order with the Unicode Bidirectional
//!    Algorithm (`unicode-bidi`), so a left-to-right painter shows it right:
//!    Arabic runs reversed, Latin words and numbers kept readable, brackets
//!    mirrored.
//!
//! Strings without RTL characters come back untouched (borrowed), so other
//! languages pay one char scan and nothing else.

use std::borrow::Cow;
use std::sync::OnceLock;

use ar_reshaper::ArabicReshaper;
use unicode_bidi::{BidiInfo, Level};

/// True for chars from right-to-left scripts (Hebrew, Arabic, Syriac,
/// Thaana, N'Ko, ... and the Arabic presentation forms).
fn is_rtl_char(c: char) -> bool {
    matches!(c as u32,
        0x0590..=0x08FF
        | 0xFB1D..=0xFDFF
        | 0xFE70..=0xFEFF
        | 0x10800..=0x10FFF
        | 0x1E800..=0x1EFFF)
}

/// Does `s` contain any right-to-left text?
pub fn has_rtl(s: &str) -> bool {
    s.chars().any(is_rtl_char)
}

/// Text from data (station names, song titles, tags): base direction comes
/// from the first strong character, as in a browser with `dir="auto"`.
pub fn visual(s: &str) -> Cow<'_, str> {
    convert(s, None)
}

/// Text from the UI catalogs. In an RTL language the paragraph is RTL even
/// when it starts with a number or a Latin word ("128 kbps ...").
pub fn visual_ui(s: &str, rtl_base: bool) -> Cow<'_, str> {
    convert(s, rtl_base.then(Level::rtl))
}

/// Owned convenience for call sites that build `String`s anyway.
pub fn visual_string(s: String) -> String {
    match visual(&s) {
        Cow::Borrowed(_) => s,
        Cow::Owned(v) => v,
    }
}

fn reshaper() -> &'static ArabicReshaper {
    static R: OnceLock<ArabicReshaper> = OnceLock::new();
    R.get_or_init(ArabicReshaper::default)
}

fn convert(s: &str, base: Option<Level>) -> Cow<'_, str> {
    if !has_rtl(s) {
        return Cow::Borrowed(s);
    }
    let shaped = reshaper().reshape(s);
    let mut out = String::with_capacity(shaped.len());
    for (i, line) in shaped.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        visual_line(line, base, &mut out);
    }
    Cow::Owned(out)
}

/// One line (no `\n`) in logical order -> visual order, appended to `out`.
fn visual_line(line: &str, base: Option<Level>, out: &mut String) {
    if line.is_empty() {
        return;
    }
    let info = BidiInfo::new(line, base);
    for para in &info.paragraphs {
        let (levels, runs) = info.visual_runs(para, para.range.clone());
        for run in runs {
            let text = &line[run.clone()];
            if levels[run.start].is_rtl() {
                out.extend(text.chars().rev().map(mirror));
            } else {
                out.push_str(text);
            }
        }
    }
}

/// Line-break logical `text` for `width` *before* reordering, then convert
/// each line. egui would otherwise wrap the already-reordered string from
/// the left and show the end of an RTL sentence on the first line. Text
/// without RTL characters is returned as is (egui wraps it normally).
pub fn wrapped(ui: &egui::Ui, text: &str, font: &egui::FontId, width: f32, rtl_base: bool) -> String {
    if !has_rtl(text) {
        return text.to_owned();
    }
    let base = rtl_base.then(Level::rtl);
    let measure = |s: &str| {
        let shown = convert(s, base).into_owned();
        ui.fonts(|f| f.layout_no_wrap(shown, font.clone(), egui::Color32::WHITE).size().x)
    };
    let lines = break_lines(text, width, measure);
    lines.iter().map(|l| convert(l, base).into_owned()).collect::<Vec<_>>().join("\n")
}

/// Greedy word wrap of logical text; `measure` gives a line's width.
fn break_lines(text: &str, width: f32, measure: impl Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        for word in para.split(' ') {
            let cand = if cur.is_empty() { word.to_owned() } else { format!("{cur} {word}") };
            if !cur.is_empty() && measure(&cand) > width {
                lines.push(std::mem::replace(&mut cur, word.to_owned()));
            } else {
                cur = cand;
            }
        }
        lines.push(cur);
    }
    lines
}

/// Bidi_Mirrored pairs that show up in UI text: inside an RTL run the
/// glyph for "(" must point the other way.
fn mirror(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_presentation_form(c: char) -> bool {
        matches!(c as u32, 0xFB50..=0xFDFF | 0xFE70..=0xFEFF)
    }

    #[test]
    fn non_rtl_text_is_borrowed_untouched() {
        for s in ["Now Playing", "正在播放", "अभी चल रहा है", "Español 128 kbps", ""] {
            assert!(matches!(visual(s), Cow::Borrowed(x) if x == s), "{s}");
            assert!(matches!(visual_ui(s, true), Cow::Borrowed(x) if x == s), "{s}");
        }
    }

    #[test]
    fn detects_rtl() {
        assert!(has_rtl("راديو"));
        assert!(has_rtl("Radio שלום"));
        assert!(!has_rtl("Radio 1"));
    }

    #[test]
    fn arabic_letters_are_joined_and_reversed() {
        // "سلام" = seen, lam, alef, meem. Joined: initial seen, lam-alef
        // ligature (final), isolated meem. Visual order is right-to-left,
        // so the string comes back last letter first.
        let v = visual("سلام");
        assert_eq!(v, "\u{FEE1}\u{FEFC}\u{FEB3}");
        assert!(v.chars().all(is_presentation_form));
    }

    #[test]
    fn word_order_is_reversed_but_words_stay_whole() {
        // "راديو عربي": two words. Visually the first word sits on the right.
        let v = visual("راديو عربي");
        let words: Vec<&str> = v.split(' ').collect();
        assert_eq!(words.len(), 2);
        assert_eq!(words[1], visual("راديو"));
        assert_eq!(words[0], visual("عربي"));
    }

    #[test]
    fn latin_and_numbers_stay_left_to_right_inside_arabic() {
        // "استمع إلى BBC بجودة 128 kbps"
        let v = visual_ui("استمع إلى BBC بجودة 128 kbps", true);
        // Latin words and digits are never reversed...
        assert!(v.contains("BBC") && v.contains("128") && v.contains("kbps"), "{v}");
        // ...the RTL paragraph puts the logical start ("استمع") on the right
        // and the trailing number/unit on the left. As in a browser with
        // dir="rtl", the space after a number in RTL context is RTL, so the
        // unit lands left of the number.
        assert!(v.ends_with(&*visual("استمع")), "{v}");
        assert_eq!(v, format!("kbps 128 {} BBC {} {}", visual("بجودة"), visual("إلى"), visual("استمع")));
    }

    #[test]
    fn auto_direction_for_latin_first_names() {
        // A Latin-first station name keeps an LTR base: "Radio" stays left,
        // and the number after the Arabic word belongs to it (UBA W2/N1).
        let v = visual("Radio سلام 99");
        assert_eq!(v, format!("Radio 99 {}", visual("سلام")));
        // Arabic-first: RTL base, the Latin word on the visual left.
        let v = visual("سلام FM");
        assert_eq!(v, format!("FM {}", visual("سلام")));
    }

    #[test]
    fn brackets_are_mirrored_in_rtl_runs() {
        // "مرحبا (عالم)": logically "(" opens before عالم. Visually, in an
        // RTL paragraph the bracket glyph that opens it sits on the right
        // of the word and must be drawn as ")"-shaped -> "(" on the left.
        let v = visual("مرحبا (عالم)");
        assert!(v.starts_with('('), "{v}");
        assert!(v.contains(')'), "{v}");
        let open = v.find('(').unwrap();
        let close = v.find(')').unwrap();
        assert!(open < close, "{v}");
    }

    #[test]
    fn lines_are_reordered_independently() {
        let v = visual("سلام\nعالم");
        let lines: Vec<&str> = v.split('\n').collect();
        assert_eq!(lines, [visual("سلام"), visual("عالم")]);
    }

    #[test]
    fn wraps_in_logical_order_before_reordering() {
        // Width counts chars here: each line keeps whole words, in reading
        // order, so the first line holds the sentence's first words.
        let lines = break_lines("واحد اثنان ثلاثة أربعة\n\nخمسة", 12.0, |s| s.chars().count() as f32);
        assert_eq!(lines, ["واحد اثنان", "ثلاثة أربعة", "", "خمسة"]);
    }

    #[test]
    fn hebrew_is_reordered_without_reshaping() {
        assert_eq!(visual("שלום"), "םולש");
    }
}
