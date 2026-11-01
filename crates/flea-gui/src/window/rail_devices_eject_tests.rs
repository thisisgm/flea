//! SidebarRow.qml's eject mark: only Eject.releasable rows trade their mount square for it, and its hit box lifts the ink.
use super::*;

fn row_grounds(
    producer: &Entity<Producer>,
    index: usize,
    cx: &mut VisualTestContext,
) -> (Vec<gpui::Background>, crate::list::Palette) {
    let (rect, palette) = producer.read_with(cx, |producer, _| {
        let laid = producer.laid.borrow();
        (laid.row(index).unwrap().rect, laid.palette.unwrap())
    });
    let grounds = cx.update(|window, _| {
        let bounds = gpui::Bounds::new(
            gpui::point(gpui::px(rect.x), gpui::px(rect.y)),
            gpui::size(gpui::px(rect.width), gpui::px(rect.height)),
        )
        .scale(window.scale_factor());
        window
            .painted_quads()
            .into_iter()
            .filter(|quad| quad.bounds == bounds)
            .map(|quad| quad.background)
            .collect()
    });
    (grounds, palette)
}

#[gpui::test]
fn rail_devices_eject_keyboard_focus_greys_only_the_row_ground(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(crate::palette::Theme::from_text(
            "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'",
        ));
    });
    let (shell, service, cx) = open("{}", cx);
    ready(&service, cx);
    let producer = producer(&shell, cx);
    let index = producer.read_with(cx, |producer, _| {
        producer
            .laid
            .borrow()
            .rows
            .iter()
            .find(|row| row.label == "Archive")
            .unwrap()
            .index
    });
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    producer.update(cx, |producer, cx| producer.set_cursor(index, cx));
    frame(cx);
    let lit = producer.read_with(cx, |producer, _| producer.model.borrow().lit);

    for (key, mark_focused) in [
        ("", false),
        ("right", true),
        ("left", false),
        ("right", true),
        ("escape", false),
    ] {
        if !key.is_empty() {
            cx.simulate_keystrokes(key);
            frame(cx);
        }
        producer.read_with(cx, |producer, _| {
            let model = producer.model.borrow();
            assert_eq!(model.mark_focused, mark_focused, "after {key:?}");
            assert_eq!(model.cursor, Some(index), "mark focus keeps the cursor row");
            assert_eq!(model.lit, lit, "mark focus keeps the current place");
            assert!(model.focused && !model.pointer_focus);
        });
        let (grounds, palette) = row_grounds(&producer, index, cx);
        assert_ne!(
            palette.accent, palette.foreground,
            "the fixture distinguishes cursor grounds"
        );
        let expected = if mark_focused {
            palette.foreground.opacity(crate::list::GREY_CURSOR)
        } else {
            palette.accent.opacity(crate::list::CURSOR_WASH)
        };
        assert_eq!(grounds, [gpui::Background::from(expected)], "after {key:?}");
        let hit = producer.read_with(cx, |producer, _| {
            producer
                .laid
                .borrow()
                .row(index)
                .unwrap()
                .eject_hit()
                .unwrap()
        });
        let framed = cx.update(|window, _| {
            let bounds = gpui::Bounds::new(
                gpui::point(gpui::px(hit.x), gpui::px(hit.y)),
                gpui::size(gpui::px(hit.width), gpui::px(hit.height)),
            )
            .scale(window.scale_factor());
            window
                .painted_quads()
                .iter()
                .any(|quad| quad.bounds == bounds && quad.border_color == palette.foreground)
        });
        assert_eq!(framed, mark_focused, "the eject frame follows mark focus");
    }
}

#[gpui::test]
fn rail_devices_releasable_rows_draw_the_eject_mark_in_the_square_slot(cx: &mut TestAppContext) {
    let (shell, service, cx) = open("{}", cx);
    ready(&service, cx);
    let producer = producer(&shell, cx);
    let slots = producer.read_with(cx, |producer, _| {
        producer
            .laid
            .borrow()
            .rows
            .iter()
            .filter(|row| row.indicator_visible)
            .map(|row| (row.label.clone(), row.eject))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        slots,
        [
            ("Archive".to_string(), true),
            ("Backup".to_string(), false),
            ("Photos".to_string(), false),
            ("Pixel 8".to_string(), true),
        ],
        "a mounted removable volume and a mounted phone release; unmounted rows keep the square and the machine row has no slot"
    );
    let (index, hit) = producer.read_with(cx, |producer, _| {
        let laid = producer.laid.borrow();
        let row = laid.rows.iter().find(|row| row.label == "Archive").unwrap();
        (row.index, row.eject_hit().unwrap())
    });
    assert!(
        hit.width >= 24. && hit.height >= 24.,
        "the hit box keeps the WCAG floor"
    );
    cx.simulate_mouse_move(
        gpui::point(
            gpui::px(hit.x + hit.width / 2.),
            gpui::px(hit.y + hit.height / 2.),
        ),
        None,
        gpui::Modifiers::none(),
    );
    frame(cx);
    assert_eq!(
        producer.read_with(cx, |producer, _| producer.model.borrow().mark_hovered),
        Some(index),
        "the pointer on the mark lifts its ink to the foreground"
    );
    let (grounds, palette) = row_grounds(&producer, index, cx);
    assert_eq!(
        grounds,
        [gpui::Background::from(
            palette.foreground.opacity(crate::list::HOVER)
        )],
        "hovering the eject mark keeps the row's hover wash"
    );
    cx.simulate_mouse_move(
        gpui::point(gpui::px(hit.x - 40.), gpui::px(hit.y + hit.height / 2.)),
        None,
        gpui::Modifiers::none(),
    );
    frame(cx);
    assert_eq!(
        producer.read_with(cx, |producer, _| producer.model.borrow().mark_hovered),
        None,
        "off the mark the ink returns to the muted frame"
    );
    assert_eq!(
        row_grounds(&producer, index, cx).0,
        grounds,
        "row hover stays unchanged off the mark"
    );
}
