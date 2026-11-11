use super::*;

pub(super) fn present(
    card: &Card,
    palette: &Palette,
    typeface: &Typeface,
    corner: f32,
    room: f32,
    live: Option<&Live<'_>>,
    probe: Option<&crate::ipc::card_geometry::CardGeometry>,
) -> AnyElement {
    let s = f32::from(typeface.body) / BOARD_BODY;
    let mut draw = Draw {
        corner: crate::compositor::valid_corner(f64::from(corner)),
        t: Tokens::of(palette),
        s,
        typeface,
        language: &card.language,
        probe,
        wire: live.map(|live| live.wire),
        mount: live.and_then(|live| live.mount),
        measure: live.and_then(|live| live.measure),
    };
    if let Some(surface) = live.and_then(|live| live.surface) {
        draw.t.surface = surface;
    }
    let t = draw.t;
    let facts = super::super::facts::is_card(card);
    let scroll = live.and_then(|live| live.scroll);
    let bounded = facts || scroll.is_some();
    let head = card_heading(
        &card.title,
        &card.esc,
        draw.text(TITLE)
            .font_weight(FontWeight::BOLD)
            .text_color(t.fg),
        draw.text(CAPTION)
            .font_weight(FontWeight::NORMAL)
            .text_color(t.muted),
        draw.px(ROW_GAP),
    )
    .when(bounded, |d| d.flex_none());
    // The buttons close the frame; a measured card also ends on its own tracking canvas.
    let tail = 1 + usize::from(draw.measure.is_some());
    let frame = div()
        .on_children_prepainted({
            let probe = probe.cloned();
            move |children, _, _| {
                if let (Some(probe), Some(bounds)) = (
                    probe.as_ref(),
                    children
                        .get(2..children.len().saturating_sub(tail))
                        .and_then(|body| body.iter().copied().reduce(|a, b| a.union(&b))),
                ) {
                    probe.probe.record("body", bounds);
                }
            }
        })
        .id("op3-frame")
        .relative()
        .when(bounded, |d| d.max_h_full())
        .debug_selector(|| "op3-card".into())
        .w(px(scaled(card.width, f32::from(typeface.body), room)))
        .flex()
        .flex_col()
        .gap(draw.px(GAP))
        .p(draw.px(PAD))
        .bg(t.surface)
        .border_1()
        .border_color(t.muted_frame)
        .rounded(px(draw.corner))
        .font_family(draw.typeface.font.family.clone())
        .text_size(draw.px(BOARD_BODY))
        .line_height(relative(1.2))
        .occlude()
        .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(head)
        .child(draw.track(
            draw.hairline().when(card.bleed, |d| d.mx(draw.px(-PAD))),
            "titleRule",
        ));
    if let Some(probe) = probe {
        probe.clear();
    }
    let frame = if scroll.is_some() {
        frame
    } else {
        draw.card_body(card, frame)
    };
    let mut buttons = div()
        .when(bounded, |d| d.flex_none())
        .mt(draw.px(ABOVE_BUTTONS))
        .flex()
        .justify_end()
        .items_center()
        .gap(draw.px(ROW_GAP));
    for (at, button) in card.buttons.iter().enumerate() {
        buttons = buttons.child(
            draw.wired(
                Control::Button(at),
                draw.button(button)
                    .id(("op3-button", at))
                    .debug_selector(move || format!("op3-button-{at}")),
            ),
        );
    }
    let frame = if let Some(scroll) = scroll {
        let body = draw
            .card_body(
                card,
                div()
                    .id("permissions-content")
                    .flex()
                    .flex_col()
                    .gap(draw.px(GAP)),
            )
            .flex_none()
            .child(buttons);
        let viewport = div()
            .id("permissions-scroll")
            .relative()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(scroll)
            .child(body);
        let viewport = match probe {
            Some(probe) => {
                let probe = probe.clone();
                viewport.child(
                    gpui::canvas(
                        move |_, window, _| {
                            probe.probe.record("scroll", window.content_mask().bounds)
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .left_0()
                    .top_0()
                    .size_full(),
                )
            }
            None => viewport,
        };
        frame.child(viewport)
    } else {
        frame.child(buttons)
    };
    div()
        .size_full()
        .when(bounded, |d| d.p(draw.px(GAP)))
        .flex()
        .items_center()
        .justify_center()
        .when_some(probe, |root, probe| {
            root.on_children_prepainted(probe.probe.first_child("card"))
        })
        .child(draw.track(frame, "card"))
        .into_any_element()
}
