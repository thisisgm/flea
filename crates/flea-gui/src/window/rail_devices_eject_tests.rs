//! SidebarRow.qml's eject mark: only Eject.releasable rows trade their mount square for it, and its hit box lifts the ink.
use super::*;

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
}
