//! The collision card draws the committed board and records its painted controls.
use super::geometry::Metrics;
use super::model::Dialog;
use super::press::Press;
use super::view::{button, caption, mark, shell, state, Scene};
use flea::ui_logic_op1::{card::Kind, collide};
use gpui::{div, prelude::*, px};

pub(super) fn draw(dialog: &Dialog, scene: &Scene, window: (f32, f32)) -> gpui::Div {
    let Dialog::Collide {
        title,
        names,
        more,
        merge,
        focus,
    } = dialog
    else {
        unreachable!()
    };
    let (palette, catalog) = (scene.palette, scene.catalog);

    // The names and the count of the rest sit together; the explanation takes the card's gap below them.
    let m = Metrics::new(scene.typeface);
    let mut list = div().flex().flex_col();
    for (at, name) in names.iter().enumerate() {
        list = list.child(
            div()
                .debug_selector(move || format!("op1-name-{at}"))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(m.name_gap))
                .child(mark(&name.icon, name.dir, m.name_mark, palette.foreground))
                .child(div().flex_1().min_w_0().truncate().child(name.name.clone())),
        );
    }
    if !more.is_empty() {
        list = list.child(
            div()
                .text_size(px(m.caption))
                .truncate()
                .child(more.clone()),
        );
    }
    let body = vec![
        list.into_any_element(),
        scene
            .track(
                caption(scene, collide::explain(catalog), palette.foreground)
                    .relative()
                    .debug_selector(|| "op1-explain".into()),
                "explain",
                collide::explain(catalog),
            )
            .into_any_element(),
    ];
    let buttons = collide::shown(*merge)
        .into_iter()
        .map(|b| {
            button(
                scene,
                Press::Collide(b),
                catalog.tr(b.label_key()),
                state(
                    b == collide::PRIMARY,
                    b == *focus,
                    b == collide::DESTRUCTIVE,
                    false,
                ),
            )
        })
        .collect();
    shell(scene, Kind::Collide, window, title.clone(), body, buttons)
}
