//! Journal invalidation revokes both receipt formats without renewing their notice clock.
use super::{draw, line, settle, start, state, Duration, TestAppContext};

#[gpui::test]
fn ey477_invalidated_journal_is_not_revived_by_the_old_receipt(cx: &mut TestAppContext) {
    for split in [false, true] {
        let (handle, _received) = start(cx, 1);
        line(
            cx,
            handle,
            r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copied 1 item","done":1,"total":1,"state":"done","undo":true}}"#,
        );
        line(
            cx,
            handle,
            r#"{"t":"transferdone","id":12,"ok":1,"failed":0}"#,
        );
        settle(cx, handle);
        if split {
            handle
                .update(cx, |root, _, cx| {
                    root.shell.update(cx, |shell, cx| {
                        shell.status_messages(cx, |messages, now| {
                            messages.say(
                                "Copied 1 item",
                                false,
                                "",
                                flea::ui_logic_status_feedback::status::Hint::TransientUndo,
                                now,
                            );
                        });
                    })
                })
                .unwrap();
            draw(cx, handle);
        }
        let before = state(cx, handle);
        assert_eq!(before.get("undoAvailable").unwrap().as_bool(), Some(true));
        let notice = before.get("notice").unwrap().as_str().unwrap();
        assert_eq!(notice, "Copied 1 item · z undoes");
        assert_eq!(
            before
                .get("transferCard")
                .unwrap()
                .get("undo")
                .unwrap()
                .get("label")
                .unwrap()
                .as_str(),
            Some("z undoes"),
            "both receipt formats paint the footer's own undo target"
        );
        assert_eq!(
            before
                .get("transferCard")
                .unwrap()
                .get("undo")
                .unwrap()
                .get("enabled")
                .unwrap()
                .as_bool(),
            Some(true),
            "both receipt formats expose the journal's undo target"
        );
        cx.executor().advance_clock(Duration::from_millis(1000));
        line(
            cx,
            handle,
            r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copied 1 item","done":1,"total":1,"state":"done","undo":false}}"#,
        );
        settle(cx, handle);
        let after = state(cx, handle);
        assert_eq!(after.get("undoAvailable").unwrap().as_bool(), Some(false));
        assert_eq!(after.get("notice").unwrap().as_str(), Some("Copied 1 item"));
        assert_eq!(
            after
                .get("transferCard")
                .unwrap()
                .get("undo")
                .unwrap()
                .get("enabled")
                .unwrap()
                .as_bool(),
            Some(false)
        );
        cx.executor().advance_clock(Duration::from_millis(
            flea::ui_logic_status_feedback::messages::MESSAGE_MS - 1000,
        ));
        cx.run_until_parked();
        draw(cx, handle);
        assert_eq!(state(cx, handle).get("notice").unwrap().as_str(), Some(""));
    }
}
