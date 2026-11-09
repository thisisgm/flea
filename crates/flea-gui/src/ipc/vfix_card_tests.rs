//! Reader regressions cover settled card state as well as the initial mounted surface.
use super::readers3_tests::{json, open, read, update};
use flea::jsondoc::{self, Json};
use gpui::{AppContext, TestAppContext, VisualTestContext};

#[gpui::test]
fn vf_readers_compress_reports_matched_fields_without_their_contents(cx: &mut TestAppContext) {
    use crate::g2::archive_flash::{
        compress::{Compress, Stop},
        keys::Key,
    };
    let handle = open(cx);
    update(cx, handle, |shell, _, cx| {
        let (mut card, _) = Compress::open(
            vec!["/files/item".into()],
            "/files",
            "files",
            1,
            &["7z", "zip"],
        );
        card.key(
            &shell.locale.catalog,
            Key::Text("private-vfix-secret".into()),
        );
        card.focus(Stop::Confirm);
        card.key(
            &shell.locale.catalog,
            Key::Text("private-vfix-secret".into()),
        );
        shell.operations.archive.compress = Some(card);
        cx.notify();
    });
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    let state = json(cx, handle, "archiveCardsState");
    assert_eq!(
        state.get("compressOpened").and_then(Json::as_bool),
        Some(true)
    );
    assert_eq!(state.get("compressCount").and_then(Json::as_f64), Some(1.));
    assert_eq!(
        state.get("compressFormat").and_then(Json::as_str),
        Some("7z")
    );
    assert_eq!(state.get("canCompress").and_then(Json::as_bool), Some(true));
    let capture = state.get("compressCapture").unwrap();
    assert_eq!(
        capture.get("encryptShown").and_then(Json::as_bool),
        Some(true)
    );
    assert!(!capture
        .get("eye")
        .and_then(Json::as_str)
        .unwrap()
        .is_empty());
    assert!(!read(cx, handle, "archiveCardsState", &[])
        .stdout
        .contains("private-vfix-secret"));
}

#[gpui::test]
fn vf_readers_properties_folder_tail_tracks_the_real_contents_walk(cx: &mut TestAppContext) {
    let handle = open(cx);
    update(cx, handle, |shell, window, cx| {
        let capture = crate::g2::context_menus::join::Captured {
            side: 0,
            listing: 1,
            rows: vec![0],
            cursor: 0,
            path: "/files".into(),
            identity: String::new(),
            id: 1,
            ready: true,
            kinds: vec![],
            background: false,
            target: None,
        };
        shell.open_properties_card(&capture, window, cx);
        let reply = jsondoc::parse(r#"{"id":1,"op":"properties","ok":true,"directory":true,"path":"/files/folder","bytes":4096}"#).unwrap();
        shell
            .properties
            .as_mut()
            .unwrap()
            .card
            .reply(&shell.locale.catalog, &reply);
    });
    let counting = json(cx, handle, "menuDialogState");
    let tail = counting
        .get("liveLines")
        .unwrap()
        .get("size")
        .unwrap()
        .get("tail")
        .and_then(Json::as_str)
        .unwrap();
    assert!(
        !tail.is_empty(),
        "an unfinished folder walk still says it is counting"
    );
    update(cx, handle, |shell, _, _| {
        let reply = jsondoc::parse(
            r#"{"id":1,"op":"contents","ok":true,"bytes":8000,"items":2,"done":true}"#,
        )
        .unwrap();
        shell
            .properties
            .as_mut()
            .unwrap()
            .card
            .reply(&shell.locale.catalog, &reply);
    });
    let finished = json(cx, handle, "menuDialogState");
    assert_eq!(
        finished
            .get("liveLines")
            .unwrap()
            .get("size")
            .unwrap()
            .get("tail")
            .and_then(Json::as_str),
        Some("")
    );
    assert_eq!(
        finished
            .get("facts")
            .unwrap()
            .get("path")
            .and_then(Json::as_str),
        Some("/files/folder")
    );
}

#[gpui::test]
fn vf_readers_permissions_reports_the_painted_checkbox_focus(cx: &mut TestAppContext) {
    use crate::g2::open_with_properties::permissions::{host::View, Permissions, Stop};
    let handle = open(cx);
    update(cx, handle, |shell, window, cx| {
        let (model, _) = Permissions::open(1, vec!["/files/item".into()], "Text");
        let catalog = shell.locale.catalog.clone();
        let view = cx.new(|cx| View::new(model, catalog, 4., cx));
        view.update(cx, |view, cx| {
            let id = 1;
            let reply = jsondoc::parse(&format!(r#"{{"id":{id},"op":"inspect","ok":true,"path":"/files/item","mode":"0644","directory":false}}"#)).unwrap();
            view.reply(&reply, cx);
            view.model.focus = Stop::Box(2);
            view.focus(window, cx);
            cx.notify();
        });
        shell.permissions.view = Some(view);
        cx.notify();
    });
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    let state = json(cx, handle, "permissionsState");
    let controls = state.get("controls").and_then(Json::as_array).unwrap();
    let checkbox = controls
        .iter()
        .find(|control| control.get("bit").and_then(Json::as_f64) == Some(64.))
        .unwrap();
    assert_eq!(checkbox.get("focused").and_then(Json::as_bool), Some(true));
    assert_eq!(checkbox.get("checked").and_then(Json::as_bool), Some(false));
    assert_eq!(checkbox.get("enabled").and_then(Json::as_bool), Some(true));
    assert!(!checkbox
        .get("centre")
        .and_then(Json::as_str)
        .unwrap()
        .is_empty());
}

#[gpui::test]
fn vf_readers_permissions_reports_the_painted_mixed_selection(cx: &mut TestAppContext) {
    use crate::g2::open_with_properties::permissions::{host::View, Permissions, Stop};
    let handle = open(cx);
    let mut mixed_note = String::new();
    update(cx, handle, |shell, window, cx| {
        let (model, _) = Permissions::open(
            1,
            vec!["/files/first".into(), "/files/second".into()],
            "Text",
        );
        let catalog = shell.locale.catalog.clone();
        mixed_note = catalog.tr("dialogs.file.permissions.mixed");
        let view = cx.new(|cx| View::new(model, catalog, 4., cx));
        view.update(cx, |view, cx| {
            for (at, mode) in ["0644", "0444"].iter().enumerate() {
                let id =
                    flea::ui_logic_open_with_properties::permissions::INSPECT_STRIDE + at as u64;
                let reply = jsondoc::parse(&format!(
                    r#"{{"id":{id},"op":"inspect","ok":true,"mode":"{mode}"}}"#,
                ))
                .unwrap();
                view.reply(&reply, cx);
            }
            view.model.focus = Stop::Box(1);
            view.focus(window, cx);
            cx.notify();
        });
        shell.permissions.view = Some(view);
        cx.notify();
    });
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    let state = json(cx, handle, "permissionsState");
    assert_eq!(
        state.get("paths").and_then(Json::as_array).unwrap().len(),
        2
    );
    assert_eq!(state.get("busy").and_then(Json::as_bool), Some(false));
    assert_eq!(
        state.get("displayedSummary").and_then(Json::as_str),
        Some(mixed_note.as_str())
    );
    let controls = state.get("controls").and_then(Json::as_array).unwrap();
    let checkbox = controls
        .iter()
        .find(|control| control.get("bit").and_then(Json::as_f64) == Some(128.))
        .unwrap();
    assert_eq!(checkbox.get("mixed").and_then(Json::as_bool), Some(true));
    assert_eq!(checkbox.get("focused").and_then(Json::as_bool), Some(true));
    assert!(!checkbox
        .get("rect")
        .and_then(Json::as_str)
        .unwrap()
        .is_empty());
}
