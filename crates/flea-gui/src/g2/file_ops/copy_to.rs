//! Copy To draws its field and buttons in the shared card at the live type scale.
use super::{
    geometry::Metrics,
    glyph::Tracked,
    model::{CopyFocus, Operations},
    press::Press,
    view::{button, state, Scene},
};
use flea::ui_logic_file_ops::card::{self, Kind};
use gpui::{div, prelude::*, px, Div};

#[allow(clippy::too_many_arguments)]
pub(super) fn card(
    scene: &Scene,
    window: (f32, f32),
    moving: bool,
    path: &str,
    focus: CopyFocus,
    busy: bool,
    error: &str,
    send: Option<&crate::g2::rclone_send::seams::Card>,
) -> Div {
    let (palette, catalog) = (scene.palette, scene.catalog);
    let m = Metrics::new(scene.typeface);
    let in_field = focus == CopyFocus::Field;
    // The chooser's path opens selected on the 24% accent wash; an empty field shows only the caret.
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
    let field = scene.track(field, "copy-field", path.to_owned());
    let field = match &scene.copy_field {
        Some(view) => super::field::mount(view.clone(), field, scene.typeface).into_any_element(),
        None => field.into_any_element(),
    };
    // The field name keeps its source case in the catalog and draws in capitals, as the boards' CSS does.
    let tracked = Tracked {
        text: send
            .map_or_else(
                || catalog.tr("jobs.copyto.field"),
                |card| card.field_label.clone(),
            )
            .to_uppercase()
            .into(),
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
    let error = send
        .and_then(|card| card.error.clone())
        .or_else(|| (!error.is_empty()).then(|| error.to_owned()));
    if let Some(error) = error {
        body.push(super::view::caption(scene, error, palette.error).into_any_element());
    }
    let confirm = send.map_or_else(
        || {
            catalog.tr(if moving {
                "jobs.button.move"
            } else {
                "jobs.button.copy"
            })
        },
        |card| card.primary.clone(),
    );
    let buttons = vec![
        button(
            scene,
            Press::CopyCancel,
            send.map_or_else(
                || catalog.tr("jobs.button.cancel"),
                |card| card.cancel.clone(),
            ),
            state(false, focus == CopyFocus::Cancel, false, false),
        ),
        button(
            scene,
            Press::CopyConfirm,
            confirm,
            state(
                true,
                focus == CopyFocus::Confirm,
                false,
                busy || send.map_or(!Operations::copy_enabled(path), |card| !card.can_submit),
            ),
        ),
    ];
    let title = send.map_or_else(
        || {
            catalog.tr(if moving {
                "jobs.copyto.moveto"
            } else {
                "jobs.copyto.title"
            })
        },
        |card| card.title.clone(),
    );
    super::view::shell(scene, Kind::CopyTo, window, title, body, buttons)
}
