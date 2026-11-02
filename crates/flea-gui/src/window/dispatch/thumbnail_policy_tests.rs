//! Real selection requests and delayed producer replies through Shell acceptance and IPC.
use super::previewtypes_tests as host;
use crate::{state::Shell, window::selection_preview::Frame};
use flea::jsondoc::{self, Json};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn current<R>(shell: &Entity<Shell>, cx: &VisualTestContext, f: impl FnOnce(&Frame) -> R) -> R {
    shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).unwrap().read(cx);
        f(preview.pending.as_ref().or(preview.drawn.as_ref()).unwrap())
    })
}

fn reply(ask: &str, file: &str) -> String {
    let document = jsondoc::parse(ask).unwrap();
    let tags = ["thumbToken", "thumbPath", "thumbListing"]
        .into_iter()
        .filter_map(|key| {
            document.get(key).map(|value| {
                let value = match value {
                    Json::Str(value) => format!("\"{}\"", flea::json::escape(value)),
                    Json::Num(value) => value.clone(),
                    _ => panic!("request identity field"),
                };
                format!(",\"{key}\":{value}")
            })
        })
        .collect::<String>();
    format!(
        r#"{{"t":"thumbed","row":0,"file":"{}","ms":0.000{tags}}}"#,
        flea::json::escape(file)
    )
}

#[gpui::test]
fn policy_reader_old_same_row_reply_cannot_set_new_frame_source(cx: &mut TestAppContext) {
    let (shell, cx) = host::open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![host::file("old.png", "image-x-generic", "Image", 0o100644)],
        1,
    );
    let old_lines = host::local(&shell, cx, "/files");
    let old_ask = tagged_request(&old_lines).to_owned();
    let old_generation = current(&shell, cx, |frame| frame.generation);
    // A delayed producer line can arrive after the owner has mounted a replacement request at the same row.
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![host::file("new.png", "image-x-generic", "Image", 0o100644)],
        2,
    );
    host::local(&shell, cx, "/files");
    current(&shell, cx, |frame| {
        assert_eq!(frame.subject.row.name, "new.png");
        assert_ne!(frame.generation, old_generation);
        assert!(frame.source.is_none());
    });
    host::answer(&shell, cx, &reply(&old_ask, "/cache/old.png"));
    current(&shell, cx, |frame| {
        assert!(
            frame.source.is_none(),
            "old reply changed current Frame.source: {:?}",
            frame.source
        );
        assert!(
            !frame.thumb_answered,
            "old reply answered the replacement frame"
        );
    });
}

fn state(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Json {
    jsondoc::parse(&crate::window::u07_tests::read(
        shell,
        cx,
        "thumbnailPolicyState",
        &[],
    ))
    .unwrap()
}

fn tagged_request(lines: &[String]) -> &str {
    lines
        .iter()
        .rev()
        .find(|line| line.contains("thumbToken") && line.contains(r#""c":"thumb""#))
        .expect("real tagged request")
}

fn decoded(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        host::pump(cx);
        if current(shell, cx, |frame| frame.image.is_some() || frame.failed) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "actual image decoder did not settle"
        );
        std::thread::yield_now();
    }
}

#[gpui::test]
fn policy_reader_column_raw_empty_and_failed_decode_stay_distinct(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/images");
    let picture = format!("{directory}/transfer.png");
    let (shell, cx) = host::open(cx, directory);
    super::test_shell::land(
        &shell,
        cx,
        directory,
        vec![host::file(
            "transfer.png",
            "image-x-generic",
            "Image",
            0o100644,
        )],
        1,
    );
    let lines = host::local(&shell, cx, directory);
    assert_eq!(
        state(&shell, cx).get("files").unwrap().get("0"),
        Some(&Json::Null)
    );
    host::meta(&shell, cx, &lines, r#","w":2,"h":2"#);
    host::answer(&shell, cx, &reply(tagged_request(&lines), &picture));
    assert_eq!(
        state(&shell, cx)
            .get("files")
            .unwrap()
            .get("0")
            .and_then(Json::as_str),
        Some(picture.as_str())
    );
    decoded(&shell, cx);
    let ready = state(&shell, cx);
    assert_eq!(ready.get("previewReady"), Some(&Json::Bool(true)));
    assert_eq!(
        ready.get("previewPath").and_then(Json::as_str),
        Some(picture.as_str())
    );
    assert_eq!(ready.get("previewIndex"), Some(&Json::Num("0".into())));
    // Existing lifecycle makes a new request; an empty backend outcome decodes the original but stays raw empty.
    super::test_shell::land(
        &shell,
        cx,
        directory,
        vec![host::file(
            "transfer.png",
            "image-x-generic",
            "Image",
            0o100644,
        )],
        2,
    );
    let lines = host::local(&shell, cx, directory);
    host::meta(&shell, cx, &lines, r#","w":2,"h":2"#);
    host::answer(&shell, cx, &reply(tagged_request(&lines), ""));
    decoded(&shell, cx);
    let original = state(&shell, cx);
    assert_eq!(
        original
            .get("files")
            .unwrap()
            .get("0")
            .and_then(Json::as_str),
        Some("")
    );
    assert_eq!(
        original.get("previewReady"),
        Some(&Json::Bool(true)),
        "original decoder ready does not invent a cached file"
    );
    super::test_shell::land(
        &shell,
        cx,
        directory,
        vec![host::file(
            "transfer.png",
            "image-x-generic",
            "Image",
            0o100644,
        )],
        3,
    );
    let lines = host::local(&shell, cx, directory);
    host::meta(&shell, cx, &lines, r#","w":2,"h":2"#);
    let missing = format!("{directory}/missing-policy.png");
    host::answer(&shell, cx, &reply(tagged_request(&lines), &missing));
    decoded(&shell, cx);
    current(&shell, cx, |frame| {
        assert!(frame.failed);
        assert!(frame.source.is_none());
    });
    let failed = state(&shell, cx);
    assert_eq!(
        failed.get("files").unwrap().get("0").and_then(Json::as_str),
        Some(missing.as_str())
    );
    assert_eq!(failed.get("previewReady"), Some(&Json::Bool(false)));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_view(crate::window::startup::View::List, window, cx)
        })
    });
    host::pump(cx);
    let hidden = state(&shell, cx);
    assert_eq!(hidden.get("previewPath").and_then(Json::as_str), Some(""));
    assert_eq!(hidden.get("previewReady"), Some(&Json::Bool(false)));
}

#[gpui::test]
fn policy_reader_tagged_frame_rejects_missing_foreign_and_retired_identity(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = host::open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![host::file(
            "photo.png",
            "image-x-generic",
            "Image",
            0o100644,
        )],
        1,
    );
    let lines = host::local(&shell, cx, "/files");
    let valid = reply(tagged_request(&lines), "/cache/current.png");
    let document = jsondoc::parse(tagged_request(&lines)).unwrap();
    let token = document.get("thumbToken").and_then(Json::as_str).unwrap();
    for wrong in [
        r#"{"t":"thumbed","row":0,"file":"/cache/legacy.png"}"#.to_owned(),
        valid.replace(
            &format!(r#""thumbToken":"{token}""#),
            r#""thumbToken":"foreign-owner""#,
        ),
        valid.replace("/files/photo.png", "/foreign/photo.png"),
        valid.replace(r#""thumbListing":1"#, r#""thumbListing":999"#),
        valid.replace(r#""row":0"#, r#""row":1"#),
    ] {
        host::answer(&shell, cx, &wrong);
        current(&shell, cx, |frame| {
            assert!(!frame.thumb_answered);
            assert!(frame.source.is_none());
        });
        assert_eq!(
            state(&shell, cx)
                .get("files")
                .unwrap()
                .get("0")
                .and_then(Json::as_str),
            Some("/cache/legacy.png"),
            "legacy lender accepted its own reply; tagged foreign copies cannot replace it"
        );
    }
    host::answer(&shell, cx, &valid);
    assert_eq!(
        state(&shell, cx)
            .get("files")
            .unwrap()
            .get("0")
            .and_then(Json::as_str),
        Some("/cache/current.png")
    );
    let preview = shell.read_with(cx, |shell, _| shell.selection_preview(0).unwrap());
    cx.update(|window, cx| preview.update(cx, |view, cx| view.hide(window, cx)));
    host::answer(&shell, cx, &valid);
    let closed = state(&shell, cx);
    assert_eq!(closed.get("previewReady"), Some(&Json::Bool(false)));
    assert_eq!(closed.get("previewPath").and_then(Json::as_str), Some(""));
    assert_ne!(
        closed.get("files").unwrap().get("0").and_then(Json::as_str),
        Some("/cache/current.png")
    );
}

#[gpui::test]
fn policy_reader_replaced_preview_owner_cannot_reuse_live_request_token(cx: &mut TestAppContext) {
    let (shell, cx) = host::open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![host::file(
            "photo.png",
            "image-x-generic",
            "Image",
            0o100644,
        )],
        1,
    );
    let old_lines = host::local(&shell, cx, "/files");
    let old_request = tagged_request(&old_lines).to_owned();
    let old_owner = shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().preview.take().unwrap()
    });
    let new_lines = host::local(&shell, cx, "/files");
    let old = jsondoc::parse(&old_request).unwrap();
    let new = jsondoc::parse(tagged_request(&new_lines)).unwrap();
    assert_ne!(
        old.get("thumbToken"),
        new.get("thumbToken"),
        "live owner replacement must not collide"
    );
    host::answer(&shell, cx, &reply(&old_request, "/cache/old-owner.png"));
    current(&shell, cx, |frame| {
        assert!(!frame.thumb_answered);
        assert!(frame.source.is_none());
    });
    assert_eq!(
        state(&shell, cx).get("files").unwrap().get("0"),
        Some(&Json::Null)
    );
    cx.update(|window, cx| old_owner.update(cx, |view, cx| view.hide(window, cx)));
}

#[gpui::test]
fn policy_reader_pending_column_keeps_current_row_cache_source_without_preview_ready(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let (shell, cx) = host::open(cx, "/files");
    super::test_shell::land(
        &shell,
        cx,
        "/files",
        vec![host::file(
            "photo.png",
            "image-x-generic",
            "Image",
            0o100644,
        )],
        1,
    );
    let lines = host::local(&shell, cx, "/files");
    assert!(
        lines
            .iter()
            .filter_map(|line| jsondoc::parse(line).ok())
            .any(|doc| doc.get("c").and_then(Json::as_str) == Some("thumb")
                && doc.get("thumbToken").is_none()),
        "actual unrelated lender requested this row"
    );
    let picture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/images/transfer.png"
    );
    host::answer(
        &shell,
        cx,
        &format!(r#"{{"t":"thumbed","row":0,"file":"{picture}"}}"#),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        host::pump(cx);
        if shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            host.list.borrow().pictures.ready(&host.pane.held, 0)
        }) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "actual lender decoder did not settle"
        );
        std::thread::yield_now();
    }
    current(&shell, cx, |frame| assert!(!frame.thumb_answered));
    let pending = state(&shell, cx);
    assert_eq!(
        pending
            .get("files")
            .unwrap()
            .get("0")
            .and_then(Json::as_str),
        Some(picture)
    );
    assert_eq!(pending.get("previewReady"), Some(&Json::Bool(false)));
}
