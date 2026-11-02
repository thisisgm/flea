//! The keyboard delivery snapshot reads the same owners the window's input routing uses.
use super::Adapter;
use crate::{state::Shell, window::startup::View};
use flea::{
    ipc::wire::Reply,
    jsondoc::{self, Json},
    ui_logic_sr1::Mode,
};
use gpui::{App, Window};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) struct KeyboardState<'a> {
    pub shell: &'a Shell,
    pub window: &'a Window,
    pub cx: &'a App,
}

/// Whether the open PDF holds the keyboard, as PdfViewer.activeFocus reads in v0.3.12.
pub(crate) fn pdf_focused(shell: &Shell, window: &Window, cx: &App) -> bool {
    KeyboardState { shell, window, cx }.focus_type() == "PdfViewer"
}

fn strings<'a>(values: impl Iterator<Item = &'a str>) -> Json {
    Json::Arr(values.map(|value| Json::Str(value.into())).collect())
}

impl KeyboardState<'_> {
    fn rename_focused(&self) -> bool {
        self.shell.focus.is_focused(self.window)
            && self
                .shell
                .rename
                .edit
                .as_ref()
                .is_some_and(|edit| edit.entry.editing)
    }

    // Comparison rule: type part only, without a QObject address or generated QML type identity.
    pub(crate) fn focus_type(&self) -> &'static str {
        if self.window.focused(self.cx).is_none() {
            return "null";
        }
        if self
            .shell
            .settings
            .panel
            .as_ref()
            .is_some_and(|view| view.read(self.cx).focus.is_focused(self.window))
            || self
                .shell
                .settings
                .sheet
                .as_ref()
                .is_some_and(|view| view.read(self.cx).focus.is_focused(self.window))
        {
            return "QQuickItem";
        }
        if let Some(overlay) = self.shell.operation_view(self.shell.focus_side) {
            let overlay = overlay.read(self.cx);
            if overlay
                .focus_handle()
                .is_some_and(|focus| focus.is_focused(self.window))
            {
                use crate::g2::op1::model::{CopyFocus, Dialog};
                return match overlay.model.dialog {
                    Some(
                        Dialog::CopyTo {
                            focus: CopyFocus::Field,
                            ..
                        }
                        | Dialog::SendTo {
                            focus: CopyFocus::Field,
                            ..
                        }
                        | Dialog::NewFile {
                            focus: CopyFocus::Field,
                            ..
                        },
                    ) => "QQuickTextInput",
                    Some(
                        Dialog::CopyTo { .. } | Dialog::SendTo { .. } | Dialog::NewFile { .. },
                    ) => "DialogButton",
                    Some(Dialog::Collide { .. }) => "CollideConfirm",
                    Some(Dialog::Fat { .. }) => "FatNamesConfirm",
                    Some(Dialog::Delete { .. }) => "TrashConfirm",
                    None => "QQuickItem",
                };
            }
        }
        if self.shell.path_holds_keyboard(self.window, self.cx) || self.rename_focused() {
            return "QQuickTextInput";
        }
        if let Some(look) = self
            .shell
            .quick_look
            .as_ref()
            .map(|host| host.view.read(self.cx))
        {
            if look
                .subject()
                .and_then(|subject| subject.body.doc())
                .is_some_and(|doc| matches!(doc.view, crate::g2::pv1::doc::View::Pdf(_)))
            {
                return "PdfViewer";
            }
        }
        if self.shell.panes[self.shell.focus_side]
            .as_ref()
            .and_then(|host| host.preview.as_ref())
            .is_some_and(|view| view.read(self.cx).focus.is_focused(self.window))
        {
            return "SelectionPreview";
        }
        if matches!(
            self.shell.focus_view(self.window, self.cx),
            "rail" | "chrome" | "strip"
        ) {
            return "QQuickItem";
        }
        if self.shell.focus.is_focused(self.window)
            && self.shell.panes[self.shell.focus_side]
                .as_ref()
                .is_some_and(|host| host.search.owner.typing() || host.filter.owner.typing())
        {
            return "QQuickTextInput";
        }
        match self.shell.view {
            View::List | View::Dual => "List",
            View::Grid => "GridArea",
            View::Columns => "ColumnsArea",
        }
    }

    fn state(&self) -> Option<Json> {
        let host = self.shell.panes[self.shell.focus_side].as_ref()?;
        let number = |value: u64| Json::Num(value.to_string());
        let text = |value: &str| Json::Str(value.into());
        let pending = self.shell.keyboard.pending();
        let identity = pending.map_or_else(String::new, |(_, seat)| {
            let view = if seat.view == View::Grid as u64 {
                "grid"
            } else if seat.view == View::Columns as u64 {
                "columns"
            } else {
                "list"
            };
            format!(
                "[\"{}\",{},{},\"{view}\"]",
                flea::json::escape(self.shell.keyboard.sequence_path()),
                seat.cursor,
                seat.selection
            )
        });
        let stamp = self
            .shell
            .operations
            .fileops
            .armed_at_side(self.shell.focus_side);
        let armed = if stamp == 0 {
            0
        } else {
            SystemTime::now()
                .checked_sub(Duration::from_millis(host.pane.now().saturating_sub(stamp)))
                .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |at| at.as_millis() as u64)
        };
        let clipboard = self.shell.clipboard.board.current();
        let search = &host.search.owner.session;
        let preview = host.preview.as_ref().map(|preview| preview.read(self.cx));
        let preview_focus = preview.is_some_and(|view| view.focus.is_focused(self.window));
        let preview_index = preview
            .filter(|view| view.visible && host.drawn_view == Some(View::Columns))
            .and_then(|view| view.pending.as_ref().or(view.drawn.as_ref()))
            .map_or(-1, |frame| frame.subject.index as i64);
        let focus_type = self.focus_type();
        let stop = self.shell.focus_view(self.window, self.cx);
        let pane_focus = self.window.focused(self.cx).is_some()
            && !self.shell.input_blocked(self.cx)
            && !self.shell.quick_look_open(self.cx)
            && !self.shell.path_holds_keyboard(self.window, self.cx)
            && (matches!(stop, "list" | "rail") || preview_focus || self.rename_focused());
        let list_focus = pane_focus
            && stop == "list"
            && !host.search.owner.typing()
            && !host.filter.owner.typing()
            && !preview_focus
            && !self.rename_focused();
        Some(Json::Obj(vec![
            (
                "keySequence".into(),
                text(pending.map_or("", |(action, _)| action)),
            ),
            ("keySequenceIdentity".into(), text(&identity)),
            ("trashArmedAt".into(), number(armed)),
            (
                "clipboard".into(),
                Json::Obj(vec![
                    (
                        "paths".into(),
                        strings(clipboard.paths.iter().map(String::as_str)),
                    ),
                    ("cut".into(), Json::Bool(clipboard.moving)),
                ]),
            ),
            (
                "searchMode".into(),
                text(match search.mode {
                    Mode::Off => "",
                    Mode::Typing => "typing",
                    Mode::Results => "results",
                }),
            ),
            (
                "filterTyping".into(),
                Json::Bool(host.filter.owner.typing()),
            ),
            ("filterQuery".into(), text(host.filter.owner.query())),
            ("searchRunning".into(), Json::Bool(search.running)),
            ("searchQuery".into(), text(&search.query)),
            ("searchScanned".into(), number(search.scanned as u64)),
            ("searchCancelled".into(), Json::Bool(search.cancelled)),
            ("paneFocus".into(), Json::Bool(pane_focus)),
            ("listFocus".into(), Json::Bool(list_focus)),
            ("activeFocusItem".into(), text(focus_type)),
            (
                "preview".into(),
                Json::Obj(vec![
                    ("index".into(), Json::Num(preview_index.to_string())),
                    ("focused".into(), Json::Bool(preview_focus)),
                ]),
            ),
            (
                "history".into(),
                Json::Obj(vec![
                    (
                        "back".into(),
                        strings(
                            host.nav
                                .history
                                .back
                                .iter()
                                .map(|entry| entry.path.as_str()),
                        ),
                    ),
                    (
                        "forward".into(),
                        strings(
                            host.nav
                                .history
                                .forward
                                .iter()
                                .map(|entry| entry.path.as_str()),
                        ),
                    ),
                ]),
            ),
        ]))
    }
}

impl Adapter for KeyboardState<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        if function != "keyDeliveryState" {
            return None;
        }
        Some(Reply::value(compact(&self.state()?)))
    }
}

fn compact(value: &Json) -> String {
    let mut output = String::new();
    let (mut quoted, mut escaped) = (false, false);
    for character in jsondoc::render(value).chars() {
        if character == '"' && !escaped {
            quoted = !quoted;
        }
        if quoted || !character.is_ascii_whitespace() {
            output.push(character);
        }
        escaped = quoted && character == '\\' && !escaped;
    }
    output
}
