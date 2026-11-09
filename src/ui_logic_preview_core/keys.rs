// What the open preview does with a key action: a pure decision the host carries out.

// Left/Right's seek step on media.
pub const SEEK_MS: i64 = 5000;
// The zoom ladder: the bottom rung fits the frame, one step a press, up to four times.
pub const ZOOM_MIN: f64 = 1.0;
pub const ZOOM_MAX: f64 = 4.0;
pub const ZOOM_STEP: f64 = 0.25;

pub fn zoom_step(zoom: f64, steps: i32) -> f64 { (zoom + steps as f64 * ZOOM_STEP).clamp(ZOOM_MIN, ZOOM_MAX) }

// The open preview's kind, as far as a key decision reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Surface {
    pub is_compare: bool,
    pub is_media: bool,
    pub is_pdf: bool,
    pub is_document: bool,
    pub html_rendered: bool,
    pub has_bar: bool,
    pub close_focused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    // The listing cursor moves and the preview follows the new row.
    Follow(i32),
    ScrollDocument(i32),
    Close,
    TogglePlay,
    CopyPatch,
    TurnPage(i32),
    Seek(i64),
    Zoom(i32),
    ZoomFit,
    ToggleExpand,
    ToggleMute,
    ToggleMarkdownView,
    ToggleWrap,
    CompareFocus(bool),
    ToggleCloseFocus,
    // A key that does nothing else still reveals the media strip.
    Nothing,
}

// Space a second time closes every kind (decided 2026-09-11); Escape always closes.
pub fn act(action: &str, s: Surface) -> Command {
    match action {
        "cursorDown" | "cursorUp" => {
            let direction = if action == "cursorDown" { 1 } else { -1 };
            if s.html_rendered {
                Command::ScrollDocument(direction)
            } else if s.is_compare {
                // A Compare overlay names two files, so the cursor moving under it opens no other file.
                Command::Nothing
            } else {
                Command::Follow(direction)
            }
        }
        "preview" | "escape" => Command::Close,
        "playPause" if s.is_media => Command::TogglePlay,
        "copyPatch" if s.is_compare => Command::CopyPatch,
        "seekBack" if s.is_pdf => Command::TurnPage(-1),
        "seekBack" => Command::Seek(-SEEK_MS),
        "seekForward" if s.is_pdf => Command::TurnPage(1),
        "seekForward" => Command::Seek(SEEK_MS),
        "parent" => Command::TurnPage(-1),
        "pageForward" => Command::TurnPage(1),
        "zoomOut" => Command::Zoom(-1),
        "zoomIn" => Command::Zoom(1),
        "zoomFit" => Command::ZoomFit,
        "expand" => Command::ToggleExpand,
        "mute" => Command::ToggleMute,
        "markdownView" if s.is_document => Command::ToggleMarkdownView,
        "toggleWrap" => Command::ToggleWrap,
        "focusNext" | "focusPrevious" if s.is_compare => Command::CompareFocus(action == "focusPrevious"),
        "focusNext" | "focusPrevious" if s.has_bar => Command::ToggleCloseFocus,
        "open" if s.has_bar && s.close_focused => Command::Close,
        _ => Command::Nothing,
    }
}

// Quick Look's PDF strip: a focus walk over its enabled, visible controls, starting unpicked at -1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Control {
    pub enabled: bool,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfCommand {
    Focus(i32),
    Activate(usize),
    TurnPage(i32),
    Zoom(i32),
    ToggleExpand,
    Scroll(i32),
    Nothing,
}

pub fn pdf_action(action: &str, controls: Option<&[Control]>, index: i32) -> PdfCommand {
    let Some(controls) = controls else { return column_pdf_action(action) };
    match action {
        "focusNext" | "focusPrevious" => {
            let step = if action == "focusPrevious" { -1 } else { 1 };
            let len = controls.len() as i32;
            let mut next = if index < 0 { if step > 0 { -1 } else { 0 } } else { index };
            for _ in 0..len {
                next = (next + step + len) % len;
                let c = controls[next as usize];
                if c.enabled && c.visible {
                    return PdfCommand::Focus(next);
                }
            }
            PdfCommand::Nothing
        }
        "open" | "preview" => match usize::try_from(index).ok().and_then(|i| controls.get(i).map(|c| (i, c))) {
            Some((i, c)) if c.enabled && c.visible => PdfCommand::Activate(i),
            _ => PdfCommand::Nothing,
        },
        "zoomOut" => PdfCommand::Zoom(-1),
        "zoomIn" => PdfCommand::Zoom(1),
        _ => column_pdf_action(action),
    }
}

// The column's PDF has no strip: pages scroll, and Space, Return or e opens Quick Look at the page in view.
pub fn column_pdf_action(action: &str) -> PdfCommand {
    match action {
        "open" | "preview" | "expand" => PdfCommand::ToggleExpand,
        "seekBack" | "parent" => PdfCommand::TurnPage(-1),
        "seekForward" | "pageForward" => PdfCommand::TurnPage(1),
        "cursorDown" => PdfCommand::Scroll(1),
        "cursorUp" => PdfCommand::Scroll(-1),
        _ => PdfCommand::Nothing,
    }
}
