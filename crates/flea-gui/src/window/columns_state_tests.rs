use super::host;
use crate::backend::Event;
use flea::protocol::Reply;
use gpui::VisualTestContext;

#[test]
fn u09_live_columns_empty_hero_uses_the_whole_pane_or_clear_free_area() {
    for (width, count) in [(800., 2), (1400., 3), (2100., 4), (2700., 5)] {
        let mut cx = host::context();
        let (window, _requests) = host::open_at(&mut cx, "/srv/a/b/c", width, count);
        host::change(&mut cx, window, |shell, window, cx| {
            shell.receive(
                0,
                Event::Reply(Reply::Listed {
                    path: "/srv/a/b/c".into(),
                    total: 0,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                Event::Reply(Reply::Rows {
                    start: 0,
                    listing: 4,
                    rows: Vec::new(),
                }),
                window,
                cx,
            );
            host::settle(shell, window, cx);
        });
        host::frame(&mut cx, window);
        let (pane, active) = window
            .update(&mut cx, |shell, window, _| {
                (
                    shell.geometry(window).panes[0],
                    shell.panes[0]
                        .as_ref()
                        .unwrap()
                        .columns
                        .as_ref()
                        .unwrap()
                        .area
                        .layout
                        .active_x(),
                )
            })
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), &cx);
        let slot = visual
            .debug_bounds("columns-state-slot")
            .expect("the actual state producer mounted");
        let mark = visual
            .debug_bounds("fb1-hero-mark")
            .expect("the hero mark painted");
        let start = if count <= 3 { 0. } else { active };
        assert_eq!(f32::from(slot.origin.x), pane.x + start);
        assert_eq!(f32::from(slot.size.width), pane.width - start);
        assert!((f32::from(mark.center().x - slot.center().x)).abs() < 1.);
        if count == 3 {
            assert!(
                f32::from(slot.size.width) > pane.width / 2.,
                "the hero was confined to the active column"
            );
        }
    }
}

#[test]
fn a_new_columns_limit_applies_to_the_open_columns_pane_at_once() {
    let mut cx = host::context();
    let (window, _requests) = host::open_at(&mut cx, "/srv/a/b/c", 2700., 5);
    host::change(&mut cx, window, host::settle);
    host::frame(&mut cx, window);
    for limit in [2, 4, 3] {
        host::change(&mut cx, window, |shell, window, cx| {
            let patch = flea::jsondoc::parse(&format!(r#"{{"columnsLimit":{limit}}}"#)).unwrap();
            shell.change_settings(patch, window, cx);
            host::settle(shell, window, cx);
        });
        host::frame(&mut cx, window);
        host::change(&mut cx, window, |shell, window, cx| {
            assert_eq!(
                host::call(shell, "columnCount", &[], window, cx)
                    .stdout
                    .trim(),
                limit.to_string(),
                "Settings set the limit to {limit}"
            );
        });
    }
}
