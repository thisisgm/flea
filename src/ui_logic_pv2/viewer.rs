//! Quick Look's PDF controls: the strip's six controls, keyboard focus through them, the zoom ladder and the column's keys.
use super::pdf::{ZOOM_MAX, ZOOM_MIN, ZOOM_STEP};

/// The strip in focus order: the pager first, then zoom, expand and close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Previous,
    Next,
    ZoomOut,
    ZoomIn,
    Expand,
    Close,
}

pub const CONTROLS: [Control; 6] = [
    Control::Previous,
    Control::Next,
    Control::ZoomOut,
    Control::ZoomIn,
    Control::Expand,
    Control::Close,
];

impl Control {
    /// The chrome glyph each control draws.
    pub fn glyph(self) -> &'static str {
        match self {
            Control::Previous => "chevron-left",
            Control::Next => "chevron-right",
            Control::ZoomOut => "minus",
            Control::ZoomIn => "plus",
            Control::Expand => "maximize",
            Control::Close => "x",
        }
    }

    /// The catalog key of each control's accessible name.
    pub fn name_key(self) -> &'static str {
        match self {
            Control::Previous => "document.pdf_previous_page",
            Control::Next => "document.pdf_next_page",
            Control::ZoomOut => "document.pdf_zoom_out",
            Control::ZoomIn => "document.pdf_zoom_in",
            Control::Expand => "document.pdf_expand",
            Control::Close => "document.pdf_close",
        }
    }
}

/// The keyboard actions the PDF surfaces answer, as the key table names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    FocusNext,
    FocusPrevious,
    Open,
    Preview,
    SeekBack,
    Parent,
    SeekForward,
    PageForward,
    ZoomOut,
    ZoomIn,
    Expand,
    CursorDown,
    CursorUp,
}

impl Action {
    /// The key table's action name, None for an action the PDF surfaces leave to their host.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "focusNext" => Action::FocusNext,
            "focusPrevious" => Action::FocusPrevious,
            "open" => Action::Open,
            "preview" => Action::Preview,
            "seekBack" | "previewPageBack" | "pageUp" => Action::SeekBack,
            "parent" => Action::Parent,
            "seekForward" | "previewPageForward" | "pageDown" => Action::SeekForward,
            "pageForward" => Action::PageForward,
            "zoomOut" => Action::ZoomOut,
            "zoomIn" => Action::ZoomIn,
            "expand" => Action::Expand,
            "cursorDown" => Action::CursorDown,
            "cursorUp" => Action::CursorUp,
            _ => return None,
        })
    }
}

/// Document paging aliases from the key table; cursor and media keys keep their own rules.
pub fn page_direction(action: &str) -> Option<i64> {
    match action {
        "previewPageForward" | "pageDown" => Some(1),
        "previewPageBack" | "pageUp" => Some(-1),
        _ => None,
    }
}

/// What an action asks the surface to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    None,
    Turn(i64),
    Zoom(f64),
    ToggleExpand,
    Scroll(i64),
    Close,
    /// Keyboard focus moved inside the strip.
    Focus(Option<usize>),
}

/// A zoom after `steps` presses, negative steps down, kept on the ladder.
pub fn zoom_step(zoom: f64, steps: i64) -> f64 {
    (zoom + steps as f64 * ZOOM_STEP).clamp(ZOOM_MIN, ZOOM_MAX)
}

/// Quick Look's viewer: its zoom, its expanded flag and the focused control, -1 as None, as it opens on a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Viewer {
    pub zoom: f64,
    pub expanded: bool,
    pub control: Option<usize>,
}

impl Default for Viewer {
    fn default() -> Self {
        Viewer {
            zoom: ZOOM_MIN,
            expanded: false,
            control: None,
        }
    }
}

impl Viewer {
    /// A new document is a new subject, so it opens fitted with no control picked, however the last one was left.
    pub fn subject_changed(&mut self) {
        self.zoom = ZOOM_MIN;
        self.control = None;
    }

    /// Whether a control is drawn: the pager only with pages, and its chevrons only past one page.
    pub fn visible(control: Control, count: usize) -> bool {
        match control {
            Control::Previous | Control::Next => count > 1,
            Control::Close => false,
            _ => true,
        }
    }

    /// Whether a control takes a press now.
    pub fn enabled(&self, control: Control, page: usize, count: usize) -> bool {
        match control {
            Control::Previous => page > 0,
            Control::Next => page + 1 < count,
            Control::ZoomOut => count > 0 && self.zoom > ZOOM_MIN,
            Control::ZoomIn => count > 0 && self.zoom < ZOOM_MAX,
            Control::Expand | Control::Close => true,
        }
    }

    /// A press of a control.
    pub fn activate(&mut self, control: Control) -> Effect {
        match control {
            Control::Previous => Effect::Turn(-1),
            Control::Next => Effect::Turn(1),
            Control::ZoomOut => self.zoom_by(-1),
            Control::ZoomIn => self.zoom_by(1),
            Control::Expand => {
                self.expanded = !self.expanded;
                Effect::ToggleExpand
            }
            Control::Close => Effect::Close,
        }
    }

    pub fn zoom_by(&mut self, steps: i64) -> Effect {
        self.zoom = zoom_step(self.zoom, steps);
        Effect::Zoom(self.zoom)
    }

    /// Quick Look's keys: Tab walks the drawn, enabled controls, Enter presses the focused one, the rest turn, zoom and scroll.
    pub fn action(&mut self, action: Action, page: usize, count: usize) -> Effect {
        match action {
            Action::FocusNext | Action::FocusPrevious => {
                let step: i64 = if action == Action::FocusPrevious {
                    -1
                } else {
                    1
                };
                let len = CONTROLS.len() as i64;
                let mut next = match self.control {
                    Some(at) => at as i64,
                    None if step > 0 => -1,
                    None => 0,
                };
                for _ in 0..CONTROLS.len() {
                    next = (next + step + len) % len;
                    let control = CONTROLS[next as usize];
                    if self.enabled(control, page, count) && Viewer::visible(control, count) {
                        self.control = Some(next as usize);
                        break;
                    }
                }
                Effect::Focus(self.control)
            }
            Action::Open | Action::Preview => match self.control.map(|at| CONTROLS[at]) {
                Some(control)
                    if self.enabled(control, page, count) && Viewer::visible(control, count) =>
                {
                    self.activate(control)
                }
                _ => Effect::None,
            },
            Action::SeekBack | Action::Parent => Effect::Turn(-1),
            Action::SeekForward | Action::PageForward => Effect::Turn(1),
            Action::ZoomOut => self.zoom_by(-1),
            Action::ZoomIn => self.zoom_by(1),
            Action::Expand => self.activate(Control::Expand),
            Action::CursorDown => Effect::Scroll(1),
            Action::CursorUp => Effect::Scroll(-1),
        }
    }
}

/// One control as the strip draws it: hidden ones take no width, a disabled one draws muted at 55%.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub control: Control,
    pub visible: bool,
    pub enabled: bool,
    pub focused: bool,
}

impl Viewer {
    /// The strip in drawing order; the pager and its count follow the name, the rest sit at the right end.
    pub fn strip(&self, page: usize, count: usize) -> [Item; 6] {
        CONTROLS.map(|control| Item {
            control,
            visible: Viewer::visible(control, count),
            enabled: self.enabled(control, page, count),
            focused: self.control.map(|at| CONTROLS[at]) == Some(control),
        })
    }
}

/// The count beside the pager, one-based from the page drawn, shown only once the document has pages.
pub fn counter(drawn: usize, count: usize) -> Option<(usize, usize)> {
    (count > 0).then_some((drawn + 1, count))
}

/// Whether an office preview cut at `first` pages may run past them, so its count reads "30+" in the bar and the facts.
pub fn cut(count: usize, first: Option<usize>) -> bool {
    first.is_some_and(|n| count >= n)
}

/// The page count as drawn: "12", or "30+" for a cut office preview.
pub fn count_text(count: usize, first: Option<usize>) -> String {
    if cut(count, first) {
        format!("{count}+")
    } else {
        count.to_string()
    }
}

/// The column's PDF has no strip: pages scroll, and Space, Return or e opens Quick Look at the page in view.
pub fn column_action(action: Action) -> Effect {
    match action {
        Action::Open | Action::Preview | Action::Expand => Effect::ToggleExpand,
        Action::SeekBack | Action::Parent => Effect::Turn(-1),
        Action::SeekForward | Action::PageForward => Effect::Turn(1),
        Action::CursorDown => Effect::Scroll(1),
        Action::CursorUp => Effect::Scroll(-1),
        _ => Effect::None,
    }
}

/// The scroll offset after `delta` rows in a frame, held inside the content.
pub fn scroll_by(
    top: f64,
    delta: i64,
    row_height: f64,
    content_height: f64,
    view_height: f64,
) -> f64 {
    (top + delta as f64 * row_height)
        .min(content_height - view_height)
        .max(0.)
}

#[cfg(test)]
#[path = "viewer_tests.rs"]
mod tests;
