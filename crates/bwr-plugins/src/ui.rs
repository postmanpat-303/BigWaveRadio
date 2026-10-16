//! Simple immediate-mode widgets for `on_panel` / `on_page`.
//!
//! The script describes widgets each frame; the app renders them and reports
//! interactions back as [`UiResult`]s, which the script sees on the next
//! frame (`ui.button` returns true, `ui.slider` returns the new value, ...).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rhai::Dynamic;

use crate::draw::DrawCmd;

#[derive(Debug, Clone, PartialEq)]
pub enum Widget {
    Heading(String),
    Label(String),
    Small(String),
    Separator,
    Space(f32),
    Button(String),
    Checkbox { label: String, value: bool },
    Slider { label: String, value: f64, min: f64, max: f64 },
    Choice { label: String, value: String, options: Vec<String> },
    Canvas { height: f32, cmds: Vec<DrawCmd> },
}

/// What the user did with widget number `index` this frame.
#[derive(Debug, Clone, PartialEq)]
pub enum UiResult {
    Clicked,
    Bool(bool),
    Float(f64),
    Text(String),
}

#[derive(Default)]
pub(crate) struct UiData {
    pub widgets: Vec<Widget>,
    pub canvases: Vec<(usize, crate::api::Draw)>,
    pub results: HashMap<usize, UiResult>,
}

#[derive(Clone, Default)]
pub(crate) struct Ui(pub Rc<RefCell<UiData>>);

impl Ui {
    pub fn new(results: HashMap<usize, UiResult>) -> Self {
        Ui(Rc::new(RefCell::new(UiData { results, ..Default::default() })))
    }

    pub fn push(&self, w: Widget) -> usize {
        let mut d = self.0.borrow_mut();
        d.widgets.push(w);
        d.widgets.len() - 1
    }

    pub fn result(&self, idx: usize) -> Option<UiResult> {
        self.0.borrow_mut().results.remove(&idx)
    }

    /// Finish the frame: resolve canvases into widgets.
    pub fn finish(&self) -> Vec<Widget> {
        let mut d = self.0.borrow_mut();
        let canvases = std::mem::take(&mut d.canvases);
        for (idx, draw) in canvases {
            if let Some(Widget::Canvas { cmds, .. }) = d.widgets.get_mut(idx) {
                *cmds = std::mem::take(&mut *draw.0.borrow_mut());
            }
        }
        std::mem::take(&mut d.widgets)
    }
}

pub(crate) fn text_of(d: &Dynamic) -> String {
    if d.is_string() {
        d.clone().into_string().unwrap_or_default()
    } else {
        d.to_string()
    }
}
