//! FIX-HARNESS-READERS: thumbnailPolicyState over the row pictures fixture.
use super::*;

// EY-446: thumbnailPolicyState's files and pending are the asks the shared picture map holds, as Ipc.qml reads thumbState.file.
#[gpui::test]
fn fixreaders_ey446_thumbnail_policy_state_reads_the_asked_files(cx: &mut TestAppContext) {
    let state = |cx: &mut TestAppContext, handle: WindowHandle<Shell>| {
        let reply = handle
            .update(cx, |shell, window, cx| {
                let (answer, reply) = std::sync::mpsc::sync_channel(1);
                shell.ipc(
                    IpcEvent::Call {
                        request: Request::Call {
                            target: "flea".into(),
                            function: "thumbnailPolicyState".into(),
                            arguments: Vec::new(),
                        },
                        answer,
                    },
                    window,
                    cx,
                );
                reply.try_recv().unwrap().reply
            })
            .unwrap();
        assert_eq!(reply.code, 0);
        flea::jsondoc::parse(reply.stdout.trim()).unwrap()
    };
    for view in [View::List, View::Grid] {
        let (handle, mut lines) = open(cx, view, "{}");
        first_settle(cx, handle, &mut lines);
        let asked = state(cx, handle);
        let pending = asked
            .get("pending")
            .and_then(flea::jsondoc::Json::as_array)
            .map_or(0, <[_]>::len);
        assert!(pending > 0, "{view:?}: the first settle's asks are pending");
        assert_eq!(
            asked.get("files").and_then(|files| files.get("1")),
            Some(&flea::jsondoc::Json::Null),
            "{view:?}: an asked row waits as null"
        );
        assert!(deliver(cx, handle, &thumbed(1, PICTURE)));
        let answered = state(cx, handle);
        assert_eq!(
            answered
                .get("files")
                .and_then(|files| files.get("1"))
                .and_then(flea::jsondoc::Json::as_str),
            Some(PICTURE),
            "{view:?}: the answered row reads its cached file"
        );
        assert!(
            answered
                .get("pending")
                .and_then(flea::jsondoc::Json::as_array)
                .is_some_and(|rows| rows.iter().all(|row| row.as_f64() != Some(1.))),
            "{view:?}: an answered row is no longer pending"
        );
    }
}
