//! The Copy To and Move To card in menuDialogState, with the fields ui/Ipc.qml gives the menu actions' dialog.
use crate::g2::file_ops::{
    host::Overlay,
    measured,
    model::{CopyFocus, Dialog, Operations},
    press::Press,
};
use crate::visual::object;
use flea::jsondoc::Json;

/// The drawn Copy To card, or None while another card or no card is drawn.
pub(crate) fn menu_copy_to_state(overlay: &Overlay) -> Option<String> {
    let Some(Dialog::CopyTo {
        moving,
        path,
        focus,
        busy,
        error,
        ..
    }) = overlay.drawn_dialog()
    else {
        return None;
    };
    let frame = &overlay.frame;
    let control = |name: &'static str, press: Option<Press>, enabled: bool, focused: bool| {
        let part = match press {
            Some(press) => frame.part(&format!("button-{}", press.code())),
            None => frame.part("copy-field"),
        };
        let visible = part.as_ref().is_some_and(|part| {
            part.bounds.size.width > gpui::px(0.) && part.bounds.size.height > gpui::px(0.)
        });
        object([
            ("name", Json::Str(name.into())),
            ("primary", Json::Bool(press == Some(Press::CopyConfirm))),
            ("hoverLive", Json::Bool(press.is_some())),
            ("pressLive", Json::Bool(press.is_some())),
            (
                "label",
                Json::Str(
                    part.as_ref()
                        .map_or_else(String::new, |part| part.text.clone()),
                ),
            ),
            ("visible", Json::Bool(visible)),
            ("enabled", Json::Bool(visible && enabled)),
            ("focused", Json::Bool(visible && focused)),
            (
                "hovered",
                Json::Bool(press.is_some() && overlay.pointer.hovered == press),
            ),
            (
                "pressed",
                Json::Bool(press.is_some() && overlay.pointer.pressed == press),
            ),
            ("centre", Json::Str(measured::centre(part.as_ref()))),
            ("rect", Json::Str(measured::rect(part.as_ref()))),
        ])
    };
    let can_submit = !busy && Operations::copy_enabled(path);
    let value = object([
        ("opened", Json::Bool(true)),
        (
            "action",
            Json::Str(if *moving { "moveTo" } else { "copyTo" }.into()),
        ),
        ("busy", Json::Bool(*busy)),
        ("committing", Json::Bool(*busy)),
        ("error", Json::Str(error.clone())),
        ("value", Json::Str(path.clone())),
        ("field", Json::Str(path.clone())),
        (
            "title",
            Json::Str(
                frame
                    .part("title")
                    .map_or_else(String::new, |part| part.text),
            ),
        ),
        (
            "rect",
            Json::Str(measured::rect(frame.part("card").as_ref())),
        ),
        (
            "controls",
            Json::Arr(vec![
                control(
                    "Cancel",
                    Some(Press::CopyCancel),
                    true,
                    *focus == CopyFocus::Cancel,
                ),
                control(
                    "Submit",
                    Some(Press::CopyConfirm),
                    can_submit,
                    *focus == CopyFocus::Confirm,
                ),
                control("Field", None, !busy, *focus == CopyFocus::Field),
            ]),
        ),
        (
            "unavailable",
            Json::Str("facts, liveLines, applications".into()),
        ),
    ]);
    Some(flea::jsondoc::render(&value).trim_end().to_owned())
}
