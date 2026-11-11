//! The GVFS bridge's Starting line stands with the busy mark, then gives way to its timeout line.
use super::native11_tests::feedback_read;
use super::*;
use flea::ui_logic_device_rail::Note;
use gpui::VisualContext;

fn effect(shell: &Entity<Shell>, cx: &mut VisualTestContext, effect: Effect) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.network_service(&effect, window, cx))
    });
    frame(cx);
}

/// One reader's value without the newline its IPC reply ends with.
fn read(shell: &Entity<Shell>, cx: &mut VisualTestContext, name: &str) -> String {
    let reply = feedback_read(shell, cx, name);
    match reply.strip_suffix('\n') {
        Some(value) => value.to_string(),
        None => panic!("{name} reply lost its closing newline: {reply:?}"),
    }
}

#[gpui::test]
fn uish_misc_bridge_starting_line_gives_way_to_its_timeout(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
    cx.replace_root_view(|_, _| crate::window::root::Root {
        shell: shell.clone(),
    });
    frame(cx);
    let origin = cx.update(|_, cx| shell.update(cx, |shell, _| shell.network_origin(0)));
    let starting = Note::new("network.bridge_starting", &["Bridge fail"]);
    effect(
        &shell,
        cx,
        Effect::Starting {
            note: starting,
            origin,
        },
    );
    assert_eq!(read(&shell, cx, "stickyMessage"), "Opening Bridge fail");
    assert_eq!(read(&shell, cx, "statusBusy"), "true");
    assert_eq!(read(&shell, cx, "statusError"), "false");
    effect(&shell, cx, Effect::Settled { origin });
    assert_eq!(read(&shell, cx, "stickyMessage"), "");
    assert_eq!(read(&shell, cx, "statusBusy"), "false");
    let failed = Note::new("network.bridge_failed_timeout", &["Bridge fail"]);
    effect(
        &shell,
        cx,
        Effect::Say {
            note: failed,
            origin,
        },
    );
    assert_eq!(
        read(&shell, cx, "lastMessage"),
        "Bridge fail could not be opened because a system service did not start · waiting for the folder timed out"
    );
    assert_eq!(read(&shell, cx, "statusError"), "true");
    assert_eq!(read(&shell, cx, "statusBusy"), "false");
    assert_eq!(read(&shell, cx, "stickyMessage"), "");
}

#[gpui::test]
fn uish_misc_bridge_starting_line_follows_its_origin(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
    cx.replace_root_view(|_, _| crate::window::root::Root {
        shell: shell.clone(),
    });
    frame(cx);
    let stale = u64::MAX;
    let starting = Note::new("network.bridge_starting", &["Bridge fail"]);
    effect(
        &shell,
        cx,
        Effect::Starting {
            note: starting,
            origin: stale,
        },
    );
    assert_eq!(
        read(&shell, cx, "stickyMessage"),
        "",
        "a stale origin draws no line"
    );
    assert_eq!(read(&shell, cx, "statusBusy"), "false");
}
