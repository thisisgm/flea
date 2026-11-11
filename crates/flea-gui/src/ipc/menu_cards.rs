//! The menu action cards drawn on the operation overlay, as v0.3.12's menuDialogState described them: Delete permanently's review and the New File field card.
use crate::g2::file_ops::{
    host::Overlay,
    model::{CopyFocus, Dialog},
    press::Press,
};
use crate::visual::{number, object};
use flea::jsondoc::Json;
use flea::ui_logic_file_ops::{confirm::Source, trash::Button};
use gpui::{Bounds, Pixels};

fn rect(bounds: Option<Bounds<Pixels>>) -> String {
    bounds.map_or_else(String::new, |b| {
        [b.origin.x, b.origin.y, b.size.width, b.size.height]
            .map(|value| f32::from(value).round().to_string())
            .join(" ")
    })
}

fn centre(bounds: Option<Bounds<Pixels>>) -> String {
    bounds.map_or_else(String::new, |b| {
        format!(
            "{} {}",
            f32::from(b.origin.x + b.size.width / 2.).round(),
            f32::from(b.origin.y + b.size.height / 2.).round()
        )
    })
}

/// One card button by its measured part: what it says, where it is drawn and the pointer's state on it.
fn control(
    overlay: &Overlay,
    name: &'static str,
    press: Press,
    enabled: bool,
    focused: bool,
) -> Json {
    let part = overlay.frame.part(&format!("button-{}", press.code()));
    let bounds = part.as_ref().map(|part| part.bounds);
    let visible =
        bounds.is_some_and(|b| b.size.width > gpui::px(0.) && b.size.height > gpui::px(0.));
    object([
        ("name", Json::Str(name.into())),
        (
            "primary",
            Json::Bool(matches!(press, Press::NewFileConfirm | Press::CopyConfirm)),
        ),
        (
            "destructive",
            Json::Bool(matches!(press, Press::Delete(Button::Delete))),
        ),
        (
            "label",
            Json::Str(part.map_or_else(String::new, |part| part.text)),
        ),
        ("visible", Json::Bool(visible)),
        ("enabled", Json::Bool(visible && enabled)),
        ("focused", Json::Bool(visible && focused)),
        (
            "hovered",
            Json::Bool(visible && overlay.pointer.hovered == Some(press)),
        ),
        (
            "pressed",
            Json::Bool(visible && overlay.pointer.pressed == Some(press)),
        ),
        ("centre", Json::Str(centre(bounds))),
        ("rect", Json::Str(rect(bounds))),
    ])
}

/// New File draws v0.3.12's menu action card: the action, the field's text and focus, Cancel and the submit button; Copy To and Move To answer from `copy_to_card`.
pub(super) fn menu_field_state(overlay: &Overlay) -> Option<String> {
    let (action, text, focus, busy, error, presses, ready) = match overlay.drawn_dialog()? {
        Dialog::NewFile {
            name,
            focus,
            busy,
            error,
            ..
        } => (
            "newFile",
            name,
            *focus,
            *busy,
            error,
            (Press::NewFileCancel, Press::NewFileConfirm),
            !name.is_empty(),
        ),
        _ => return None,
    };
    let value = object([
        ("opened", Json::Bool(true)),
        ("action", Json::Str(action.into())),
        ("busy", Json::Bool(busy)),
        ("committing", Json::Bool(false)),
        ("error", Json::Str(error.clone())),
        ("field", Json::Str(text.clone())),
        (
            "rect",
            Json::Str(rect(overlay.frame.part("card").map(|part| part.bounds))),
        ),
        (
            "controls",
            Json::Arr(vec![
                control(
                    overlay,
                    "Cancel",
                    presses.0,
                    true,
                    focus == CopyFocus::Cancel,
                ),
                control(
                    overlay,
                    "Submit",
                    presses.1,
                    ready && !busy,
                    focus == CopyFocus::Confirm,
                ),
                object([
                    ("name", Json::Str("Field".into())),
                    ("focused", Json::Bool(focus == CopyFocus::Field)),
                    ("visible", Json::Bool(overlay.frame.part("newfile-field").is_some())),
                    ("enabled", Json::Bool(!busy)),
                    ("rect", Json::Str(rect(overlay.frame.part("newfile-field").map(|p| p.bounds)))),
                    ("centre", Json::Str(centre(overlay.frame.part("newfile-field").map(|p| p.bounds)))),
                ]),
            ]),
        ),
        (
            "unavailable",
            Json::Str(
                "facts, liveLines, applications, cursor, confirmation, controls[2].label, controls[2].hovered, controls[2].pressed"
                    .into(),
            ),
        ),
    ]);
    Some(flea::jsondoc::render(&value).trim_end().to_owned())
}

/// The menu Delete permanently card uses v0.3.12 state and measured bounds; Trash answers through trashState.
pub(super) fn menu_delete_state(overlay: &Overlay) -> Option<String> {
    let model = &overlay.model;
    let Source::Menu(id) = model.confirm.as_ref()?.source else {
        return None;
    };
    if model.external_delete.is_some() {
        return None;
    }
    let Some(Dialog::Delete {
        review,
        scope,
        focus,
        busy,
        ..
    }) = overlay.drawn_dialog()
    else {
        return None;
    };
    let control = |name: &'static str, button: Button| {
        let enabled = !(*busy && button == Button::Delete);
        control(
            overlay,
            name,
            Press::Delete(button),
            enabled,
            *focus == button,
        )
    };
    // v0.3.12 hands the reply's numeric token through (snapshot.token || 0, so none reads 0); a token that is not a number stays a string.
    let token = if review.token.is_empty() {
        number(0)
    } else {
        review
            .token
            .parse::<u64>()
            .map_or_else(|_| Json::Str(review.token.clone()), number)
    };
    let count = review.count.unwrap_or(0);
    let catalog = &overlay.catalog;
    let card = Json::Str(rect(overlay.frame.part("card").map(|part| part.bounds)));
    let value = object([
        ("opened", Json::Bool(true)),
        ("action", Json::Str("deletePermanently".into())),
        ("busy", Json::Bool(*busy)),
        (
            "committing",
            Json::Bool(model.confirm.as_ref().is_some_and(|confirm| !confirm.shown)),
        ),
        ("error", Json::Str(String::new())),
        (
            "facts",
            object([
                ("bytes", number(review.bytes)),
                ("count", number(count)),
                ("id", number(id)),
                ("ok", Json::Bool(true)),
                ("op", Json::Str("prepareDelete".into())),
                ("t", Json::Str("menuaction".into())),
                ("token", token.clone()),
            ]),
        ),
        ("rect", card.clone()),
        (
            "confirmation",
            object([
                ("opened", Json::Bool(true)),
                ("token", token),
                ("count", number(count)),
                ("all", Json::Bool(review.all)),
                ("destructiveFocus", Json::Bool(*focus == Button::Delete)),
                (
                    "title",
                    Json::Str(crate::g2::file_ops::view::delete_title(
                        catalog, review, *busy,
                    )),
                ),
                (
                    "body",
                    Json::Str(crate::g2::file_ops::view::delete_body(
                        catalog, review, *scope, *busy,
                    )),
                ),
                ("rect", card),
                ("cancel", control("Cancel", Button::Cancel)),
                ("danger", control("Delete", Button::Delete)),
            ]),
        ),
    ]);
    Some(flea::jsondoc::render(&value).trim_end().to_owned())
}
