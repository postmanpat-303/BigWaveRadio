//! Open/close logic for the top search bar's "Recent searches" drop-down.
//!
//! Root cause of the "picking a history entry does nothing" bug: the popup's
//! open state was recomputed every frame as `field has focus && query empty`.
//! egui's TextEdit surrenders keyboard focus on the *press* of any click
//! outside itself — including a press on an entry inside the popup. On that
//! press frame `has_focus()` became false, so the code closed the popup
//! before it was drawn; the matching *release* then landed on nothing, and
//! a click (press + release on the same widget) never registered on the entry.
//!
//! Fix: focus only *opens* the popup. Once open it stays open until an entry
//! is picked, the user types, the history empties, or egui's own
//! click-outside handling closes it.

/// Whether the history popup should be open this frame.
///
/// * `was_open` – the popup was open at the end of the previous frame
///   (egui has already applied click-outside closing to this value).
/// * `has_focus` – the search field has keyboard focus.
pub fn popup_open(was_open: bool, has_focus: bool, query_empty: bool, has_history: bool) -> bool {
    has_history && query_empty && (has_focus || was_open)
}

/// What picking `q` from the drop-down does to the search state: the query
/// is filled in and a search for it is issued immediately (no debounce),
/// and it moves to the top of the history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub query: String,
    pub run_search_now: bool,
}

pub fn pick(q: &str, history: &mut Vec<String>, max: usize) -> Pick {
    let q = q.trim().to_string();
    history.retain(|h| !h.eq_ignore_ascii_case(&q));
    history.insert(0, q.clone());
    history.truncate(max);
    Pick { query: q, run_search_now: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_opens_the_popup() {
        assert!(popup_open(false, true, true, true));
        assert!(!popup_open(false, true, true, false), "no history, no popup");
        assert!(!popup_open(false, true, false, true), "typing hides it");
    }

    /// The bug: the press on an entry steals focus from the field. The popup
    /// must survive that frame so the release completes the click.
    #[test]
    fn popup_survives_the_focus_loss_of_a_click_on_an_entry() {
        let mut open = popup_open(false, true, true, true); // field focused
        assert!(open);
        open = popup_open(open, false, true, true); // mouse pressed on entry
        assert!(open, "popup closed on press: the click could never complete");
        open = popup_open(open, false, true, true); // release frame
        assert!(open);
    }

    #[test]
    fn closes_after_click_outside_or_pick() {
        // egui's CloseOnClickOutside clears was_open; without focus it stays shut.
        assert!(!popup_open(false, false, true, true));
        // A pick fills the query, which closes it.
        assert!(!popup_open(true, false, false, true));
    }

    #[test]
    fn picking_runs_the_search_immediately_and_bumps_history() {
        let mut h = vec!["trance".to_string(), "Kiss FM".to_string(), "chill".to_string()];
        let p = pick("kiss fm", &mut h, 8);
        assert_eq!(p, Pick { query: "kiss fm".into(), run_search_now: true });
        assert_eq!(h, ["kiss fm", "trance", "chill"]);
    }
}
