use super::*;

#[gpui::test]
fn fx_inactive_drive_native_frames_without_input(cx: &mut TestAppContext) {
    let (shell, _service, cx) = root_at(cx);
    native_frames(cx);
    producer(&shell, cx).update(cx, |producer, cx| {
        producer.key_action("cursorLast", cx);
        producer.key_action("menu", cx);
    });
    let before = shell.read_with(cx, |shell, _| shell.presentation.activity.draws);
    native_frames(cx);
    let draws = shell.read_with(cx, |shell, _| shell.presentation.activity.draws) - before;
    assert_eq!(
        draws, NATIVE_FRAMES,
        "each requested menu animation frame must render Root"
    );
    // As in the other menu paint controls, reduced motion makes the revealed card fully opaque.
    cx.update(|_, cx| cx.set_reduce_motion(true));
    crate::g2::context_menus::element::PAINTED.with(|painted| painted.borrow_mut().clear());
    native_frames(cx);
    let painted = crate::g2::context_menus::element::PAINTED.with(|painted| painted.borrow().len());
    assert_eq!(
        menu_rows(&shell, cx),
        ["open", "unmountVolume", "eject", "flashImage"]
    );
    assert!(
        painted > 0,
        "device menu must paint without an input-rate bypass"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
