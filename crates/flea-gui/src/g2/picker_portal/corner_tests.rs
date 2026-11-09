//! The chooser process sets the compositor corner from Hyprland itself; no test sets the global by hand here.
use crate::compositor::tests::Fixture;
use gpui::TestAppContext;

#[gpui::test]
fn picker_gm_launch_reads_the_hyprland_rounding(cx: &mut TestAppContext) {
    let root = Fixture::new();
    root.answer(r#"{"option":"decoration:rounding","int":8,"set":true}"#);
    let command = root.command(0);
    let (service, corners) = super::launch::corner_source(None, command);
    // Blocking on the reader thread's first answer is the handshake; the chooser then follows a channel it owns alone.
    let read = corners
        .recv_blocking()
        .expect("the source reads hyprctl once");
    drop(service);
    let (send, follow) = async_channel::unbounded();
    send.send_blocking(read).unwrap();
    drop(send);
    cx.update(|cx| {
        assert_eq!(
            crate::compositor::corner(cx),
            0.,
            "square until Hyprland answers"
        );
        super::launch::follow_corner((None, follow), cx).detach();
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| crate::compositor::corner(cx)),
        8.,
        "the chooser's setup path reads rounding 8"
    );
}
