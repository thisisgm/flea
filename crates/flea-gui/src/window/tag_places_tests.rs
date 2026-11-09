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
fn pb_tags_rail_missing_colour_paints_the_theme_tag_square(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    // Latte's Red differs from its text ink, so the square's colour tells the two apart.
    let latte = crate::palette::Theme::from_text(crate::window::fixchrome_tests::LATTE);
    assert_ne!(latte.tag_menu_inks[0], latte.foreground);
    cx.update(|_, cx| cx.set_global(latte));
    let mut tag = row("Red", "");
    tag.group = Group::Tag;
    tag.kind = Kind::Tag;
    tag.tag = Some("Red");
    rows(&shell, vec![tag], cx);
    producer(&shell, cx).update(cx, |producer, cx| {
        Rc::make_mut(&mut producer.model.borrow_mut().sections)[0].rows[0].mark =
            crate::g2::rail_places::content::RowMark::Tag(
                crate::g2::rail_places::content::TagMark {
                    fill: None,
                    index: 0,
                    digit: "1",
                    fallback: true,
                },
            );
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
                    && quad.background.as_solid() == Some(theme.tag_menu_inks[0])
            }),
            "EY-493: an unresolved rail colour paints the theme's own Red square, never the text ink or a framed digit"
        );
    });
}

/// EY-493: the TAGS rail row the producer builds paints its square in the theme's own tag colour, as the listing row does.
#[gpui::test]
fn fixchrome_ey493_rail_tag_square_paints_the_tag_colour(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    for theme in [
        crate::palette::Theme::default(),
        crate::palette::Theme::from_text(crate::window::fixchrome_tests::LATTE),
    ] {
        cx.update(|_, cx| cx.set_global(theme));
        let mut tag = row("Red", "");
        tag.group = Group::Tag;
        tag.kind = Kind::Tag;
        tag.tag = Some("Red");
        rows(&shell, vec![tag], cx);
        frame(cx);
        let rect = producer(&shell, cx).read_with(cx, |producer, _| {
            producer
                .laid
                .borrow()
                .row(0)
                .expect("the tag row is drawn")
                .rect
        });
        cx.update(|window, _| {
            let scale = window.scale_factor();
            let squares: Vec<_> = window
                .painted_quads()
                .iter()
                .filter(|quad| {
                    let (x, y) = (
                        quad.bounds.origin.x.as_f32() / scale,
                        quad.bounds.origin.y.as_f32() / scale,
                    );
                    (quad.bounds.size.width.as_f32() - 14. * scale).abs() < 0.1
                        && (quad.bounds.size.height.as_f32() - 14. * scale).abs() < 0.1
                        && x >= rect.x
                        && x < rect.x + rect.width
                        && y >= rect.y
                        && y < rect.y + rect.height
                })
                .filter_map(|quad| quad.background.as_solid())
                .collect();
            assert!(
                squares.contains(&theme.tag_menu_inks[0]),
                "the rail Red square is the theme's Red: {squares:?}"
            );
            // A theme with no red of its own resolves Red to its text ink, so only a coloured Red can be told apart.
            assert!(
                theme.tag_menu_inks[0] == theme.foreground || !squares.contains(&theme.foreground),
                "no rail tag square in the text ink: {squares:?} in {rect:?}, Red {:?}, ink {:?}",
                theme.tag_menu_inks[0],
                theme.foreground
            );
        });
    }
}
