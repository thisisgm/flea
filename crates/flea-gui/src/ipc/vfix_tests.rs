//! VF-READERS regressions use the production window dispatch and mounted card owners.
use super::readers3_tests::{json, open, read, update};
use crate::g2::open_with_properties::permissions::{host::View, Permissions};
use flea::jsondoc::Json;
use gpui::{AppContext, TestAppContext, VisualTestContext};

fn opened(value: &Json) {
    assert_eq!(value.get("opened").and_then(Json::as_bool), Some(true));
}

#[gpui::test]
fn vf_readers_permissions_mounted_card_is_observable(cx: &mut TestAppContext) {
    let handle = open(cx);
    update(cx, handle, |shell, _, cx| {
        let (model, _) = Permissions::open(1, vec!["/files/test".into()], "Text");
        shell.permissions.view =
            Some(cx.new(|cx| View::new(model, shell.locale.catalog.clone(), 4., cx)));
        cx.notify();
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    let state = json(cx, handle, "permissionsState");
    opened(&state);
    assert_eq!(state.get("inputReady").and_then(Json::as_bool), Some(false));
    assert!(!state.get("rect").and_then(Json::as_str).unwrap().is_empty());
    let frame = VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("op3-card")
        .expect("the observed card is drawn");
    let drawn = crate::ipc::rows::rect_of(
        frame.origin.x.into(),
        frame.origin.y.into(),
        frame.size.width.into(),
        frame.size.height.into(),
    );
    assert_eq!(
        state.get("rect").and_then(Json::as_str),
        Some(drawn.as_str())
    );
    assert!(!state
        .get("controls")
        .and_then(Json::as_array)
        .unwrap()
        .is_empty());
    let controls = state.get("controls").and_then(Json::as_array).unwrap();
    let visible: Vec<_> = controls
        .iter()
        .filter(|control| control.get("visible").and_then(Json::as_bool) == Some(true))
        .map(|control| control.get("name").and_then(Json::as_str).unwrap())
        .collect();
    assert_eq!(visible, ["Cancel"], "the loading card only offers Cancel");
    {
        let button = controls
            .iter()
            .find(|control| control.get("name").and_then(Json::as_str) == Some("Cancel"))
            .unwrap();
        assert_eq!(button.get("name").and_then(Json::as_str), Some("Cancel"));
        assert_eq!(button.get("visible").and_then(Json::as_bool), Some(true));
        assert_eq!(
            button
                .get("centre")
                .and_then(Json::as_str)
                .unwrap()
                .split_whitespace()
                .count(),
            2,
            "{button:?}"
        );
        for key in ["hovered", "pressed"] {
            assert_eq!(button.get(key).and_then(Json::as_bool), Some(false));
        }
    }
}

#[gpui::test]
fn vf_readers_properties_reports_its_mounted_card(cx: &mut TestAppContext) {
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
    });
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    let state = json(cx, handle, "menuDialogState");
    opened(&state);
    assert_eq!(
        state.get("action").and_then(Json::as_str),
        Some("properties")
    );
    assert!(state.get("rect").and_then(Json::as_str).unwrap().is_empty());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(150));
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    let state = json(cx, handle, "menuDialogState");
    assert!(!state.get("rect").and_then(Json::as_str).unwrap().is_empty());
    let (card, body) = update(cx, handle, |shell, _, _| {
        let probe = &shell.properties.as_ref().unwrap().probe.probe;
        (probe.bounds("card").unwrap(), probe.bounds("body").unwrap())
    });
    assert!(body.size.width > gpui::px(0.) && body.size.height > gpui::px(0.));
    assert!(body.left() >= card.left() && body.right() <= card.right());
    assert!(body.top() >= card.top() && body.bottom() <= card.bottom());
    let cancel = &state.get("controls").and_then(Json::as_array).unwrap()[0];
    let centre: Vec<f32> = cancel
        .get("centre")
        .and_then(Json::as_str)
        .unwrap()
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    assert_eq!(centre.len(), 2);
    let position = gpui::point(gpui::px(centre[0]), gpui::px(centre[1]));
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_move(position, None, Default::default());
    visual.simulate_mouse_down(position, gpui::MouseButton::Left, Default::default());
    visual.run_until_parked();
    let held = json(cx, handle, "menuDialogState");
    let cancel = &held.get("controls").and_then(Json::as_array).unwrap()[0];
    assert_eq!(cancel.get("hovered").and_then(Json::as_bool), Some(true));
    assert_eq!(cancel.get("pressed").and_then(Json::as_bool), Some(true));
}

#[gpui::test]
fn vf_readers_archive_reports_only_mask_state_and_drawn_eye(cx: &mut TestAppContext) {
    let handle = open(cx);
    update(cx, handle, |shell, _, cx| {
        shell.operations.archive.extract.open(
            "/files/private.7z",
            "/files/out",
            "",
            &shell.locale.catalog,
        );
        shell
            .operations
            .archive
            .extract
            .take_secret_for_test("private-vfix-secret");
        cx.notify();
    });
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    let state = json(cx, handle, "archiveCardsState");
    opened(&state);
    let capture = state.get("extractCapture").unwrap();
    assert_eq!(capture.get("filled").and_then(Json::as_bool), Some(true));
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
fn vf_readers_unmounted_previews_return_json_objects(cx: &mut TestAppContext) {
    let handle = open(cx);
    for (function, args) in [
        ("htmlState", vec!["false"]),
        ("htmlState", vec!["true"]),
        ("previewTable", vec![]),
    ] {
        let reply = read(cx, handle, function, &args);
        assert_eq!(reply.code, 0, "{function}: {reply:?}");
        assert_eq!(
            flea::jsondoc::parse(reply.stdout.trim()).unwrap(),
            Json::Obj(vec![])
        );
    }
}

#[gpui::test]
fn vf_readers_preview_table_closed_contract_is_json(cx: &mut TestAppContext) {
    let handle = open(cx);
    assert_eq!(
        read(cx, handle, "previewTable", &[]),
        flea::ipc::wire::Reply::value("{}")
    );
}

#[gpui::test]
fn vf_readers_html_reports_the_mounted_rendered_and_source_model(cx: &mut TestAppContext) {
    use crate::g2::quick_look_overlay::doc::View;
    let handle = open(cx);
    let doc = super::reader_port_b_tests::document(cx, handle, "index.html", "text-html");
    let View::Html(view) = doc.view else {
        panic!("mounted HTML");
    };
    view.update(cx, |view, _| {
        view.doc.total = 61;
        view.doc.image_file = "/private/rendered.png".into();
        view.doc.has_drawn = true;
        view.doc.picture = flea::ui_logic_doc_preview::html::Picture::Ready;
    });
    let reply = read(cx, handle, "htmlState", &["false"]);
    assert_eq!(reply.code, 0, "{reply:?}");
    let state = flea::jsondoc::parse(&reply.stdout).unwrap();
    assert_eq!(state.get("view").and_then(Json::as_str), Some("rendered"));
    assert_eq!(state.get("lines").and_then(Json::as_f64), Some(61.));
    assert_eq!(state.get("ready").and_then(Json::as_bool), Some(true));
    view.update(cx, |view, cx| {
        view.set_view(flea::ui_logic_doc_preview::html::View::Source, cx)
    });
    let reply = read(cx, handle, "htmlState", &["false"]);
    assert_eq!(reply.code, 0, "{reply:?}");
    let state = flea::jsondoc::parse(&reply.stdout).unwrap();
    assert_eq!(state.get("view").and_then(Json::as_str), Some("source"));
    assert_eq!(
        state.get("ready").and_then(Json::as_bool),
        Some(false),
        "Source readiness belongs to its own code viewport"
    );
}

#[gpui::test]
fn vf_readers_unlock_reports_open_fields_and_mask_without_secret(cx: &mut TestAppContext) {
    let handle = open(cx);
    update(cx, handle, |shell, window, cx| {
        shell.rail = Some(crate::window::rail::mount(
            shell.locale.catalog.clone(),
            &shell.launch.settings.data,
            0,
            cx,
        ));
        shell.open_unlock(
            0,
            "/dev/unit-only",
            "Test drive",
            1024,
            "crypto_LUKS",
            window,
            cx,
        );
        shell
            .rail
            .as_ref()
            .unwrap()
            .unlock
            .as_ref()
            .unwrap()
            .view
            .update(cx, |view, cx| {
                view.host
                    .card
                    .as_mut()
                    .unwrap()
                    .field
                    .type_text("private-vfix-secret");
                cx.notify();
            });
    });
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    let state = json(cx, handle, "unlockState");
    opened(&state);
    assert_eq!(
        state.get("device").and_then(Json::as_str),
        Some("/dev/unit-only")
    );
    assert_eq!(state.get("masked").and_then(Json::as_bool), Some(true));
    for key in ["phase", "note", "label", "subtitle", "focus", "rect", "eye"] {
        assert!(state.get(key).is_some(), "{key}");
    }
    assert!(!state.get("rect").and_then(Json::as_str).unwrap().is_empty());
    assert!(!state.get("eye").and_then(Json::as_str).unwrap().is_empty());
    assert!(!read(cx, handle, "unlockState", &[])
        .stdout
        .contains("private-vfix-secret"));
}

#[gpui::test]
fn vf_readers_network_menu_reads_the_menu_opened_by_the_network_route(cx: &mut TestAppContext) {
    let handle = open(cx);
    update(cx, handle, |shell, window, cx| {
        shell.rail = Some(crate::window::rail::mount(
            shell.locale.catalog.clone(),
            &shell.launch.settings.data,
            0,
            cx,
        ));
        shell.ensure_network(window, cx);
        shell
            .rail
            .as_mut()
            .unwrap()
            .network
            .as_mut()
            .unwrap()
            .rows
            .push(flea::ui_logic_device_rail::RailEntry {
                label: "NAS".into(),
                path: "/fixture/nas".into(),
                group: flea::ui_logic_device_rail::Group::Network,
                kind: flea::ui_logic_device_rail::Kind::Share,
                mounted: true,
                uri: "smb://nas/share".into(),
                ..Default::default()
            });
        shell.network_menu(
            0,
            &flea::ui_logic_rail_places::rail::RailEntry {
                group: flea::ui_logic_rail_places::rail::Group::Network,
                path: "/fixture/nas".into(),
                uri: "smb://nas/share".into(),
                ..Default::default()
            },
            window,
            cx,
        );
        assert!(shell
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .menu
            .borrow()
            .opened());
        assert!(!shell.menu.menu.borrow().opened());
    });
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    assert_eq!(
        read(cx, handle, "contextMenuVisible", &[]),
        flea::ipc::wire::Reply::value("true")
    );
    let labels = read(cx, handle, "contextMenuEntries", &[]);
    assert_eq!(labels.code, 0);
    let expected = update(cx, handle, |shell, _, _| {
        shell.locale.catalog.tr("device.menu.open")
    });
    assert!(
        labels
            .stdout
            .trim()
            .split('|')
            .any(|label| label == expected),
        "{labels:?}"
    );
    let state = json(cx, handle, "menuState");
    assert_eq!(state.get("opened").and_then(Json::as_bool), Some(true));
    assert_eq!(state.get("forRail").and_then(Json::as_bool), Some(true));
    assert!(!read(cx, handle, "contextMenuRect", &[])
        .stdout
        .trim()
        .is_empty());
    assert!(!read(cx, handle, "contextMenuRowCentre", &["0"])
        .stdout
        .trim()
        .is_empty());
    assert!(!read(cx, handle, "contextMenuRowProbe", &["0"])
        .stdout
        .trim()
        .is_empty());
    let model = json(cx, handle, "contextMenuModel");
    assert!(model
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row.get("id").and_then(Json::as_str) == Some("open")));
}
