use super::*;

#[gpui::test]
fn m9_slow_remote_write_says_the_backend_sentence_as_a_notice(cx: &mut TestAppContext) {
    let handle = open(cx);
    frame(cx, handle);
    let line = r#"{"t":"slow","op":"rename","path":"/hung/a.txt","msg":"/hung is slow. The rename will finish on its own."}"#;
    handle
        .update(cx, |shell, window, cx| {
            shell.menu.pending = Some(("rename".into(), "a.txt".into()));
            let pending = shell.menu.pending.clone();
            assert!(
                flea::ui_logic_op1::parse(line).is_none(),
                "slow is not an operation completion"
            );
            assert!(
                !shell.menu_reply(0, line, window, cx),
                "slow is not a menu completion"
            );
            let event = crate::backend::Backend::test_reply_line(line);
            shell.receive(0, event, window, cx);
            assert_eq!(
                shell.menu.pending, pending,
                "a slow notice consumed the pending menu request"
            );
            assert!(
                shell.operation_view(0).is_none(),
                "a slow notice entered the completion path"
            );
        })
        .unwrap();
    assert_eq!(
        read(cx, handle, "lastMessage"),
        Reply::value("/hung is slow. The rename will finish on its own.")
    );
    assert_eq!(read(cx, handle, "statusError"), Reply::value("false"));
    assert_eq!(
        crate::window::events::slow_sentence(r#"{"t":"slow","op":"mkdir","msg":""}"#),
        None
    );
    assert_eq!(
        crate::window::events::slow_sentence(r#"{"t":"renamed","ok":true,"path":"/slow"}"#),
        None
    );
}
