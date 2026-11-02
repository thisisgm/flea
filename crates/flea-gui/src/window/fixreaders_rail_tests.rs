//! FIX-HARNESS-READERS: rail readers over the U08 composition fixture.
use super::*;

// EY-187: PaneRail.qml revealed follows the pointer on a docked rail, so a pointer resting on the drawn rail after a row press reads true.
#[gpui::test]
fn fixreaders_ey187_docked_rail_under_the_pointer_reads_revealed(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    rows(&shell, vec![row("A", "/a"), row("B", "/b")], cx);
    frame(cx);
    let revealed = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        let state = jsondoc::parse(&ipc(shell, "railState", &[], cx).stdout).unwrap();
        assert_eq!(
            state.get("hidden").and_then(jsondoc::Json::as_bool),
            Some(false),
            "the docked rail is on screen"
        );
        state.get("revealed").and_then(jsondoc::Json::as_bool)
    };
    assert_eq!(revealed(&shell, cx), Some(false));
    let centre = producer(&shell, cx).read_with(cx, |producer, _| {
        let laid = producer.laid.borrow();
        let row = laid
            .row(1)
            .expect("composition painted the second rail row");
        point(
            px(row.rect.x + row.rect.width / 2.),
            px(row.rect.y + row.rect.height / 2.),
        )
    });
    cx.simulate_mouse_move(centre, None, Modifiers::none());
    frame(cx);
    assert_eq!(revealed(&shell, cx), Some(true), "the pointer on the rail");
    cx.simulate_mouse_down(centre, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(centre, MouseButton::Left, Modifiers::none());
    frame(cx);
    assert_eq!(
        revealed(&shell, cx),
        Some(true),
        "a row press leaves the pointer resting on the rail"
    );
}
