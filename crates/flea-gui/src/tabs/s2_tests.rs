//! S2 tab tear refusal through the same native payload route as the strip.
use super::join_tests::{key, land, read};
use crate::{backend::Backend, state::tests::shell_with};
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn s2_int97_98_rename_tear_refuses_with_the_shipped_sentence(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        shell.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            land(shell, "/files", &[("file.txt", false)], 3, window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            host.nav.gate.editing = true;
            let now = shell.tab_now();
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 0, now));
            assert!(shell.tab_native_payload(cx).is_none());
            assert!(!shell.strip.bar.out_active);
            assert_eq!(
                read(shell, "lastMessage", window, cx).stdout,
                "Finish the rename before dragging a tab out.\n"
            );
            assert_eq!(shell.strip.tabs[0].count(), 2);
        });
    });
}
