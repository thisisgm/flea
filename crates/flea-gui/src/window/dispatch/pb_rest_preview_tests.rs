//! Mounted multi-selection, flyout motion and rested Quick Look regressions.
use super::pb_rest_tests::answer_columns;
use super::test_shell::{land, open_root, open_root_with};
use gpui::TestAppContext;

#[gpui::test]
fn pb_rest_multi_selection_draws_its_kind_summary_and_stack(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"columns","preview":{"column":true}"#, None);
    let mut text = super::preview_tests::file("notes.txt", "text-plain");
    text.mtime = 1_750_000_000;
    let mut picture = super::preview_tests::file("photo.png", "image-x-generic");
    picture.mtime = 1_750_000_060;
    land(&shell, cx, "/files", vec![text, picture], 1);
    super::search_open_tests::line(
        &shell,
        cx,
        r#"{"t":"fsinfo","path":"/files","fs":"ext4","free":1000,"class":"local"}"#,
    );
    cx.simulate_keystrokes("shift-down");
    answer_columns(&shell, cx);
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0, 1])
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1000));
    super::menus_tests::settle(cx);
    let facts = crate::window::u07_tests::read(&shell, cx, "previewFacts", &[]);
    let state = shell.read_with(cx, |shell, cx| {
        let host = shell.panes[0].as_ref().unwrap();
        (
            shell.view,
            shell.shown_view(),
            host.pane.busy(),
            host.path.clone(),
            host.pane.place().to_owned(),
            host.preview.as_ref().map(|view| {
                let view = view.read(cx);
                (
                    view.visible,
                    view.pending.as_ref().map(|f| f.kind),
                    view.drawn.as_ref().map(|f| f.kind),
                )
            }),
        )
    });
    assert!(
        facts.contains("Kinds=1 text, 1 image"),
        "the mounted multi facts: {facts}; {state:?}"
    );
    assert!(facts.contains("Combined="), "combined size is drawn");
    assert!(
        facts.contains("Newest=") && facts.contains("Oldest="),
        "the date range is drawn"
    );
    assert!(
        cx.debug_bounds("column-kind-stack").is_some(),
        "stacked-kind mark is mounted in the multi frame"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_closed_flyout_keeps_its_last_card_during_the_fade(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", super::menus_tests::home(), 1);
    super::test_shell::set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("c");
    super::menus_tests::answer(&shell, cx, &[0o100644]);
    super::preview_tests::draw(cx);
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell
                .menu
                .menu
                .borrow()
                .flyout
                .as_ref()
                .map(|flyout| flyout.action.as_str()),
            Some("copyAs")
        )
    });
    cx.simulate_keystrokes("escape");
    super::preview_tests::draw(cx);
    assert!(
        shell.read_with(cx, |shell, _| shell.menu.menu.borrow().flyout.is_none()),
        "the flyout is no longer interactive"
    );
    assert!(
        cx.debug_bounds("menu-flyout-fade").is_some(),
        "the closing flyout keeps its last rows while fading"
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(70));
    super::preview_tests::draw(cx);
    shell.read_with(cx, |shell, _| {
        let menu = shell.menu.menu.borrow();
        let alpha = menu
            .faded
            .as_ref()
            .expect("the flyout is still fading")
            .painted
            .get();
        assert!(
            alpha > 0. && alpha < 1.,
            "the painted opacity eased during the close: {alpha}"
        );
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    super::preview_tests::draw(cx);
    assert!(
        cx.debug_bounds("menu-flyout-fade").is_none(),
        "the finished fade releases its card"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_rest_builds_the_closed_quick_look_card_before_space(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![
            super::preview_tests::file("one.txt", "text-plain"),
            super::preview_tests::file("two.txt", "text-plain"),
        ],
        1,
    );
    assert!(
        shell.read_with(cx, |shell, _| shell.quick_look.is_none()),
        "an initial non-Markdown row does not warm the card"
    );
    cx.simulate_keystrokes("down");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1000));
    super::menus_tests::settle(cx);
    shell.read_with(cx, |shell, cx| {
        let host = shell
            .quick_look
            .as_ref()
            .expect("the rested file builds the card closed");
        assert!(!host.view.read(cx).is_open());
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
