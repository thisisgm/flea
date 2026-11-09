//! The preview family: Quick Look on Space and its keys while open, then the selection preview's keys on the focused pane.
use super::Order;
use crate::state::Shell;
use flea::keys::Context as Keys;
use gpui::{Context, Window};

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    let Order::Action(event) = order else {
        return false;
    };
    // Focus.js lookup: an open card owns the preview, pdf and media contexts.
    if matches!(event.context, Keys::Preview | Keys::Pdf | Keys::Media)
        && shell.quick_look_action(event, window, cx)
    {
        return true;
    }
    // SelectionPreview.qml: Space in the focused column plays media and opens Quick Look on any other file.
    if event.action == "preview" && matches!(event.context, Keys::Preview | Keys::Pdf | Keys::Media)
    {
        let side = shell.focus_side;
        let still = shell
            .selection_preview(side)
            .filter(|column| column.read(cx).focus.is_focused(window))
            .is_some_and(|column| {
                !column.read(cx).mounted_kind().is_some_and(|(kind, _)| {
                    matches!(
                        kind,
                        crate::window::selection_preview::Kind::Video
                            | crate::window::selection_preview::Kind::Audio
                    )
                })
            });
        if still && shell.open_quick_look(side, window, cx) {
            return true;
        }
    }
    if event.context == Keys::Listing && !shell.input_blocked(cx) {
        let side = shell.focus_side;
        let file = shell.panes[side]
            .as_ref()
            .filter(|host| !host.pane.busy())
            .and_then(|host| host.pane.held.row(host.pane.cursor))
            .map(|row| !row.dir);
        match (event.action, file) {
            // PreviewKeys.open: Space on a file opens Quick Look; on a folder or no loaded row it is quiet.
            ("preview", Some(true)) => return shell.open_quick_look(side, window, cx),
            ("preview", _) if super::super::quick_look::custom::enabled() => {
                return shell.open_quick_look(side, window, cx);
            }
            ("preview", _) => return true,
            // Focus.js lookup: pageForward on a file row is the preview action.
            ("pageForward", Some(true)) => return shell.open_quick_look(side, window, cx),
            _ => {}
        }
    }
    shell.selection_preview_action(shell.focus_side, event, window, cx)
}
