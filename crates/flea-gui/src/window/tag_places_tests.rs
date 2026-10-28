//! A real tag rail press opens the search session the query reader reports.
use super::*;

#[gpui::test]
fn pb_tags_rail_click_opens_tag_query(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    let mut tag = row("Red", "");
    tag.group = Group::Tag;
    tag.kind = Kind::Tag;
    tag.tag = Some("Red");
    rows(&shell, vec![tag], cx);
    let centre = producer(&shell, cx).read_with(cx, |producer, _| {
        let laid = producer.laid.borrow();
        let row = laid.row(0).expect("the tag row is drawn");
        point(
            px(row.rect.x + row.rect.width / 2.),
            px(row.rect.y + row.rect.height / 2.),
        )
    });
    cx.simulate_mouse_move(centre, None, Modifiers::none());
    cx.simulate_mouse_down(centre, MouseButton::Left, Modifiers::none());
    frame(cx);
    let reply = ipc(&shell, "keyDeliveryState", &[], cx);
    let state = flea::jsondoc::parse(reply.stdout.trim()).expect("search session JSON");
    assert_eq!(
        state
            .get("searchQuery")
            .and_then(flea::jsondoc::Json::as_str),
        Some("tag:Red")
    );
    assert_eq!(ipc(&shell, "focusView", &[], cx).stdout.trim(), "list");
}

#[gpui::test]
fn pb_tags_rail_missing_colour_paints_solid_foreground_square(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    let mut tag = row("Red", "");
    tag.group = Group::Tag;
    tag.kind = Kind::Tag;
    tag.tag = Some("Red");
    rows(&shell, vec![tag], cx);
    producer(&shell, cx).update(cx, |producer, cx| {
        Rc::make_mut(&mut producer.model.borrow_mut().sections)[0].rows[0].mark =
            crate::g2::pl4::content::RowMark::Tag(crate::g2::pl4::content::TagMark {
                fill: None,
                digit: "1",
                fallback: true,
            });
        cx.notify();
    });
    frame(cx);
    cx.update(|window, cx| {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let scale = window.scale_factor();
        assert!(
            window.painted_quads().iter().any(|quad| {
                (quad.bounds.size.width.as_f32() - 14. * scale).abs() < 0.1
                    && (quad.bounds.size.height.as_f32() - 14. * scale).abs() < 0.1
                    && quad.background.as_solid() == Some(theme.foreground)
            }),
            "a missing theme colour must paint a solid square instead of a framed digit"
        );
    });
}
