//! Focus and control receipts for the mounted path, rename, Trash and transfer owners.
use super::cohesion_controls::{control, state};
use crate::{state::Shell, visual::object};
use flea::jsondoc::Json;
use gpui::{App, Bounds, Pixels, Window};

fn source(name: &str, bounds: Bounds<Pixels>, focused: bool, enabled: bool) -> Json {
    let (x, y, w, h) = (
        bounds.origin.x.into(),
        bounds.origin.y.into(),
        bounds.size.width.into(),
        bounds.size.height.into(),
    );
    object([
        ("name", Json::Str(name.into())),
        ("rect", Json::Str(super::rows::rect_of(x, y, w, h))),
        ("centre", Json::Str(super::rows::centre_of(x, y, w, h))),
        ("visible", Json::Bool(w > 0. && h > 0.)),
        ("enabled", Json::Bool(enabled)),
        ("focused", Json::Bool(focused)),
    ])
}

pub(super) fn read(shell: &Shell, window: &Window, cx: &App) -> Option<Json> {
    if let Some(chrome) = shell.chrome.view.as_ref() {
        let path = chrome.read(cx).path.read(cx);
        if path.editing() && path.built {
            let source = source(
                "",
                path.paint.borrow().frame?,
                path.holds_keyboard(window),
                true,
            );
            return Some(state(
                control(&source, "", "PathEntry").into_iter().collect(),
            ));
        }
    }
    if let Some(edit) = shell
        .rename
        .edit
        .as_ref()
        .filter(|edit| edit.entry.editing && shell.renaming_row(edit.side).is_some())
    {
        let source = source(
            "",
            edit.line.frame?,
            edit.pending.is_none(),
            edit.pending.is_none(),
        );
        return Some(state(
            control(&source, "", "RenameField").into_iter().collect(),
        ));
    }
    if let Some(host) = &shell.trash {
        let pane = host.pane.read(cx);
        if pane.view().opened {
            let bounds = *pane.controls.borrow().get("trash-empty")?;
            let mut row = source(
                "Empty Trash",
                bounds,
                pane.focus.is_focused(window) && pane.view().strip_focus,
                pane.empty_available(),
            );
            if let Json::Obj(fields) = &mut row {
                fields.extend([
                    ("hovered".into(), Json::Bool(pane.button.hover)),
                    ("pressed".into(), Json::Bool(pane.button.pressed)),
                    ("destructive".into(), Json::Bool(true)),
                ]);
            }
            return Some(state(
                control(&row, "TrashView", "DialogButton")
                    .into_iter()
                    .collect(),
            ));
        }
    }
    let overlay = shell.overlay.as_ref()?.read(cx);
    if overlay.drawn_jobs().folded || !overlay.drawn_jobs().visible() {
        return None;
    }
    let (id, block) = overlay.model.blocks.first()?;
    use crate::g2::file_ops::press::Press;
    use flea::ui_logic_file_ops::jobs::Action;
    let entries = [
        ("Cancel", "cancel", Action::Cancel),
        ("Dismiss", "dismiss", Action::Dismiss),
    ]
    .into_iter()
    .filter_map(|(name, part, action)| {
        let text = overlay.feedback_paint.get(part)?;
        let press = Press::Job(id.id, action);
        let mut row = source(name, text.bounds, false, block.buttons.contains(&action));
        if let Json::Obj(fields) = &mut row {
            fields.extend([
                (
                    "hovered".into(),
                    Json::Bool(overlay.pointer.hovered == Some(press)),
                ),
                (
                    "pressed".into(),
                    Json::Bool(overlay.pointer.pressed == Some(press)),
                ),
            ]);
        }
        control(&row, "TransferCard", "DialogButton")
    })
    .collect();
    Some(state(entries))
}
