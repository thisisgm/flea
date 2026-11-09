//! Quick Look's keys over a Markdown document: r flips Rendered and Source, w wraps Source, the walk moves to and from the close mark,
//! open closes from it, and the cursor keys scroll.

/// The view a Markdown document is shown in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Rendered,
    Source,
}

impl View {
    /// The name ui.json and the IPC read: "rendered" or "source".
    pub fn name(self) -> &'static str {
        match self {
            View::Rendered => "rendered",
            View::Source => "source",
        }
    }

    /// "source" is Source and anything else reads as Rendered.
    pub fn from_name(name: &str) -> Self {
        if name == "source" {
            View::Source
        } else {
            View::Rendered
        }
    }

    pub fn other(self) -> Self {
        match self {
            View::Rendered => View::Source,
            View::Source => View::Rendered,
        }
    }
}

/// What a key does to the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    View(View),
    Wrap,
    /// The close mark takes or gives up the keyboard.
    CloseFocus(bool),
    Close,
    Scroll(i64),
}

/// The effect of the key table's action on a document shown in `view`, the close mark focused or not.
pub fn action(name: &str, view: View, close_focused: bool) -> Effect {
    match name {
        "markdownView" => Effect::View(view.other()),
        "toggleWrap" => Effect::Wrap,
        "focusNext" | "focusPrevious" => Effect::CloseFocus(!close_focused),
        "open" if close_focused => Effect::Close,
        "cursorDown" => Effect::Scroll(1),
        "cursorUp" => Effect::Scroll(-1),
        _ => Effect::None,
    }
}

/// A wheel stroke's sideways travel: Shift turns a vertical notch with no sideways part sideways, as it does in a browser.
pub fn across(dx: f32, dy: f32, shift: bool) -> f32 {
    if shift && dx == 0.0 {
        dy
    } else {
        dx
    }
}

/// True when a stroke goes to a table under it: its sideways travel beats its vertical, or Shift holds any sideways travel.
/// Control leaves every stroke to the document.
pub fn sideways(dx: f32, dy: f32, shift: bool, control: bool) -> bool {
    let x = across(dx, dy, shift).abs();
    if control {
        false
    } else if shift {
        x != 0.0
    } else {
        x > dy.abs()
    }
}

#[cfg(test)]
#[path = "markdown_keys_tests.rs"]
mod tests;
