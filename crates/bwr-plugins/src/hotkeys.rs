//! App-wide hotkey chords declared by plugins in `plugin.toml`:
//!
//! ```toml
//! [[hotkeys]]
//! keys = "Ctrl+F,U"     # hold Ctrl, press F, then U within ~1 s
//! action = "start"      # passed to on_hotkey(ctx, action)
//! ```

use serde::Deserialize;

/// Max time between the steps of a chord.
pub const CHORD_TIMEOUT: f64 = 1.0;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HotkeySpec {
    pub keys: String,
    pub action: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub mods: Mods,
    /// egui key name, e.g. "F", "U", "F5", "ArrowUp".
    pub key: String,
}

/// Parse "Ctrl+F,U" / "Ctrl+Shift+K" / "Alt+1,2". The first step must use
/// Ctrl or Alt so plain typing can never trigger a plugin. Later steps may
/// be pressed with or without the first step's modifiers still held.
pub fn parse_chord(s: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for part in s.split(',') {
        let mut mods = Mods::default();
        let mut key = None;
        for tok in part.split('+').map(str::trim) {
            match tok.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => mods.ctrl = true,
                "shift" => mods.shift = true,
                "alt" => mods.alt = true,
                "" => return Err(format!("hotkey \"{s}\": empty key")),
                _ if key.is_some() => return Err(format!("hotkey \"{s}\": more than one key in \"{part}\"")),
                _ => {
                    let k = if tok.len() == 1 { tok.to_ascii_uppercase() } else { tok.to_string() };
                    if k == "Escape" || k == "Esc" {
                        return Err(format!("hotkey \"{s}\": Esc is reserved"));
                    }
                    key = Some(k);
                }
            }
        }
        let key = key.ok_or_else(|| format!("hotkey \"{s}\": \"{part}\" has no key"))?;
        steps.push(Step { mods, key });
    }
    if steps.len() > 3 {
        return Err(format!("hotkey \"{s}\": at most 3 steps"));
    }
    if !(steps[0].mods.ctrl || steps[0].mods.alt) {
        return Err(format!("hotkey \"{s}\": the first key needs Ctrl or Alt"));
    }
    Ok(steps)
}

/// Tracks partially-typed chords.
#[derive(Debug, Default)]
pub struct ChordMatcher {
    /// (chord index, steps matched so far, time of last step)
    progress: Vec<(usize, usize, f64)>,
}

/// What to do with a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordResult {
    /// Not part of any chord: let the app handle it.
    Ignored,
    /// Part of a chord in progress: swallow it.
    Pending,
    /// Chord `index` completed: swallow it and fire.
    Fired(usize),
}

impl ChordMatcher {
    pub fn key(&mut self, chords: &[Vec<Step>], now: f64, key: &str, mods: Mods) -> ChordResult {
        self.progress.retain(|(_, _, t)| now - t <= CHORD_TIMEOUT);
        // Continue chords in progress.
        let mut advanced = Vec::new();
        for &(ci, n, _) in &self.progress {
            let Some(chord) = chords.get(ci) else { continue };
            let want = &chord[n];
            // Later steps accept the earlier modifiers still being held.
            let held = Mods { ctrl: mods.ctrl && !want.mods.ctrl, shift: mods.shift && !want.mods.shift, alt: mods.alt && !want.mods.alt };
            let first = &chord[0].mods;
            let mods_ok = (mods == want.mods) || (held.ctrl <= first.ctrl && held.shift <= first.shift && held.alt <= first.alt && {
                let base = Mods { ctrl: mods.ctrl && want.mods.ctrl, shift: mods.shift && want.mods.shift, alt: mods.alt && want.mods.alt };
                base == want.mods
            });
            if want.key == key && mods_ok {
                advanced.push((ci, n + 1));
            }
        }
        self.progress.clear();
        for (ci, n) in advanced {
            if n == chords[ci].len() {
                return ChordResult::Fired(ci);
            }
            self.progress.push((ci, n, now));
        }
        if !self.progress.is_empty() {
            return ChordResult::Pending;
        }
        // Start new chords.
        for (ci, chord) in chords.iter().enumerate() {
            if chord[0].key == key && chord[0].mods == mods {
                if chord.len() == 1 {
                    return ChordResult::Fired(ci);
                }
                self.progress.push((ci, 1, now));
            }
        }
        if self.progress.is_empty() {
            ChordResult::Ignored
        } else {
            ChordResult::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: Mods = Mods { ctrl: true, shift: false, alt: false };
    const NONE: Mods = Mods { ctrl: false, shift: false, alt: false };

    #[test]
    fn parses() {
        let c = parse_chord("Ctrl+F,U").unwrap();
        assert_eq!(c, vec![Step { mods: CTRL, key: "F".into() }, Step { mods: NONE, key: "U".into() }]);
        assert!(parse_chord("F,U").is_err(), "needs a modifier");
        assert!(parse_chord("Ctrl+Escape").is_err());
        assert!(parse_chord("Ctrl+F+G").is_err());
        assert!(parse_chord("Ctrl+").is_err());
    }

    #[test]
    fn matches_ctrl_f_then_u_with_or_without_ctrl() {
        let chords = vec![parse_chord("Ctrl+F,U").unwrap()];
        for umods in [CTRL, NONE] {
            let mut m = ChordMatcher::default();
            assert_eq!(m.key(&chords, 0.0, "F", CTRL), ChordResult::Pending);
            assert_eq!(m.key(&chords, 0.5, "U", umods), ChordResult::Fired(0));
        }
    }

    #[test]
    fn times_out_and_resets() {
        let chords = vec![parse_chord("Ctrl+F,U").unwrap()];
        let mut m = ChordMatcher::default();
        m.key(&chords, 0.0, "F", CTRL);
        assert_eq!(m.key(&chords, 1.5, "U", CTRL), ChordResult::Ignored);
        m.key(&chords, 2.0, "F", CTRL);
        assert_eq!(m.key(&chords, 2.2, "X", CTRL), ChordResult::Ignored);
        assert_eq!(m.key(&chords, 2.3, "U", CTRL), ChordResult::Ignored);
        assert_eq!(m.key(&chords, 3.0, "U", NONE), ChordResult::Ignored, "plain U never fires");
        assert_eq!(m.key(&chords, 3.1, "F", NONE), ChordResult::Ignored, "plain F never starts");
    }
}
