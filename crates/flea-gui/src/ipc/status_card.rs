//! Legacy transfer-card observations come from the Jobs block the operations overlay actually paints.
use crate::{
    state::Shell,
    visual::{number, object},
};
use flea::{jsondoc::Json, ui_logic_op1::jobs::Action};
use gpui::App;

// Ipc.qml controlState names the control as v0.3.12 does, whatever the painted label reads.
fn control(name: &str, text: Option<crate::g2::fb1::observe::Text>, enabled: bool) -> Json {
    let centre = text
        .as_ref()
        .map(|t| {
            let b = t.bounds;
            crate::ipc::rows::centre_of(
                f32::from(b.origin.x),
                f32::from(b.origin.y),
                f32::from(b.size.width),
                f32::from(b.size.height),
            )
        })
        .unwrap_or_default();
    let label = text
        .as_ref()
        .map(|t| t.text.to_string())
        .unwrap_or_default();
    let rect = text.as_ref().map(super::status::rect).unwrap_or_default();
    let Json::Obj(mut state) = super::status::text_state(text, 0.) else {
        unreachable!()
    };
    state.extend([
        ("name".into(), Json::Str(name.into())),
        ("label".into(), Json::Str(label)),
        ("enabled".into(), Json::Bool(enabled)),
        ("centre".into(), Json::Str(centre)),
        ("rect".into(), Json::Str(rect)),
    ]);
    Json::Obj(state)
}

pub(super) fn read(shell: &Shell, cx: &App, line: &crate::g2::fb1::line::MessageLine) -> Json {
    let overlay = shell.overlay.as_ref().map(|overlay| overlay.read(cx));
    let frame = overlay.and_then(|overlay| overlay.feedback_paint.get("frame"));
    let visible = frame.is_some()
        && overlay.is_some_and(|o| !o.drawn_jobs().folded && o.drawn_jobs().visible());
    let block = overlay
        .filter(|_| visible)
        .and_then(|o| o.model.blocks.first());
    let cancelling = block.is_some_and(|(_, block)| block.cancelling);
    let can_cancel = visible && block.is_some_and(|(_, b)| b.buttons.contains(&Action::Cancel));
    let cancel = overlay
        .filter(|_| visible)
        .and_then(|o| o.feedback_paint.get("cancel"));
    let can_dismiss = visible && block.is_some_and(|(_, b)| b.buttons.contains(&Action::Dismiss));
    let dismiss = overlay
        .filter(|_| visible)
        .and_then(|o| o.feedback_paint.get("dismiss"));
    let painted_text = |name| {
        overlay
            .filter(|_| visible)
            .and_then(|overlay| overlay.feedback_paint.get(name))
            .map(|text| text.text.to_string())
            .unwrap_or_default()
    };
    object([
        ("visible", Json::Bool(visible)),
        ("cancelling", Json::Bool(cancelling)),
        ("byteLine", Json::Str(painted_text("byte-line"))),
        ("headline", Json::Str(painted_text("headline"))),
        ("sourceLine", Json::Str(painted_text("source"))),
        (
            "writing",
            Json::Bool(overlay.is_some_and(|o| o.model.wire.transfer.writing)),
        ),
        (
            "rect",
            Json::Str(frame.as_ref().map(super::status::rect).unwrap_or_default()),
        ),
        ("cancel", control("Cancel", cancel, can_cancel)),
        ("dismiss", control("Dismiss", dismiss, can_dismiss)),
        (
            "undo",
            control(
                "z undoes",
                line.painted.get("undo"),
                !line.drawn().undo.is_empty(),
            ),
        ),
        (
            "blocks",
            number(overlay.map_or(0, |o| o.model.blocks.len())),
        ),
    ])
}
