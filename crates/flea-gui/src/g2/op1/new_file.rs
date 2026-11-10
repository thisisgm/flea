//! New File draws its name field and buttons in the shared card, as Copy To does, with v0.3.12's status line under the field.
use super::{
    geometry::Metrics,
    glyph::Tracked,
    model::CopyFocus,
    press::Press,
    view::{button, caption, state, Scene},
};
use flea::ui_logic_op1::card::{self, Kind};
use gpui::{div, prelude::*, px, Div};

/// The New File card: the name field under its label, the rule or refusal beneath, and Cancel and Create
pub(super) fn card(
    scene: &Scene,
    window: (f32, f32),
    name: &str,
    focus: CopyFocus,
    busy: bool,
    error: &str,
) -> Div {
    let (palette, catalog) = (scene.palette, scene.catalog);
    let m = Metrics::new(scene.typeface);
    let in_field = focus == CopyFocus::Field;
    // The name opens selected on the 24% accent wash; an empty name shows only the caret.
    let field = div()
        .debug_selector(|| "op1-field".into())
        .h(px(m.button))
        .px(px(m.button_pad))
        .flex()
        .items_center()
        .gap(px(m.field_gap))
        .border_1()
        .border_color(if in_field {
            palette.accent_frame
        } else {
            palette.muted_frame
        })
        .rounded(px(palette.radius))
        .bg(palette.background);
    let field = match &scene.copy_field {
        Some(view) => super::field::mount(view.clone(), field, scene.typeface).into_any_element(),
        None => field.into_any_element(),
    };
    // The field name keeps its source case in the catalog and draws in capitals, as the boards' CSS does.
    let tracked = Tracked {
        text: catalog.tr("jobs.newfile.field").to_uppercase().into(),
        size: m.caption,
        line: m.label_line,
        track: card::FIELD_LABEL_TRACK,
        ink: palette.muted,
    };
    let label = div()
        .flex()
        .debug_selector(|| "op1-field-label".into())
        .mb(px(m.label_gap))
        .child(
            div()
                .debug_selector(|| "op1-field-label-text".into())
                .child(tracked),
        );
    let mut body = vec![div()
        .flex()
        .flex_col()
        .child(label)
        .child(field)
        .into_any_element()];
    // The refusal's sentence in the error role, or the working note while create is out; nothing otherwise.
    if !error.is_empty() {
        body.push(caption(scene, error.into(), palette.error_on_surface).into_any_element());
    } else if busy {
        body.push(
            caption(scene, catalog.tr("jobs.newfile.working"), palette.muted).into_any_element(),
        );
    } else {
        // The rule the TUI's prompt states, in the card's own line under the field.
        body.push(caption(scene, catalog.tr("prompt.create"), palette.muted).into_any_element());
    }
    let buttons = vec![
        button(
            scene,
            Press::NewFileCancel,
            catalog.tr("jobs.button.cancel"),
            state(false, focus == CopyFocus::Cancel, false, busy),
        ),
        button(
            scene,
            Press::NewFileConfirm,
            catalog.tr("jobs.newfile.create"),
            state(
                true,
                focus == CopyFocus::Confirm,
                false,
                busy || name.is_empty(),
            ),
        ),
    ];
    let title = catalog.tr("jobs.newfile.title");
    super::view::shell(scene, Kind::NewFile, window, title, body, buttons)
}
