//! FIX-READERS3 (EY-228): readers v0.3.12 answers that GPUI used to refuse, reached through the production Root dispatch.
use crate::{
    backend::Backend,
    g2::open_with_properties::openwith::OpenWith,
    state::{tests::shell_with, Shell},
    window::{open_with::Open as OpenWithOpen, root::Root},
};
use flea::{
    ipc::wire::{Reply, Request},
    jsondoc::{self, Json},
};
use gpui::{AppContext, Context, TestAppContext, Window, WindowBackgroundAppearance, WindowHandle};

pub(super) fn open(cx: &mut TestAppContext) -> WindowHandle<Root> {
    let (backend, _receive) = Backend::test_channel();
    cx.add_window(|_, cx| Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    })
}

pub(super) fn update<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| change(shell, window, cx))
        })
        .unwrap()
}

pub(super) fn read(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    name: &str,
    args: &[&str],
) -> Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    let arguments = args.iter().map(|argument| (*argument).to_owned()).collect();
    update(cx, handle, |shell, window, cx| {
        shell.ipc(
            super::server::Event::Call {
                request: Request::Call {
                    target: "flea".into(),
                    function: name.into(),
                    arguments,
                },
                answer,
            },
            window,
            cx,
        )
    });
    receive.try_recv().unwrap().reply
}

fn value(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    name: &str,
    args: &[&str],
    expected: &str,
) {
    assert_eq!(
        read(cx, handle, name, args),
        Reply::value(expected),
        "{name}"
    );
}

pub(super) fn json(cx: &mut TestAppContext, handle: WindowHandle<Root>, name: &str) -> Json {
    let reply = read(cx, handle, name, &[]);
    assert_eq!(reply.code, 0, "{name}: {reply:?}");
    jsondoc::parse(reply.stdout.trim()).unwrap()
}

// An open card refuses with the painted geometry it cannot report, and names exactly those fields.
fn refused(cx: &mut TestAppContext, handle: WindowHandle<Root>, name: &str, fields: &str) {
    let reply = read(cx, handle, name, &[]);
    assert_eq!(reply.code, 3, "{name}: {reply:?}");
    assert!(
        reply.stderr.starts_with(&format!(
            "FLEA_IPC_ERROR flea.{name} unavailable fields: {fields} "
        )),
        "{reply:?}"
    );
}

#[gpui::test]
fn ipc_readers3_mode_is_always_browse(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "mode", &[], "browse");
}

#[gpui::test]
fn ipc_readers3_recent_from_is_idle_without_recent(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "recentFrom", &[], "");
}

#[gpui::test]
fn ipc_readers3_menu_dialog_state_is_closed_without_a_card(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "menuDialogState", &[], "{\"opened\":false}");
}

#[gpui::test]
fn ipc_readers3_open_with_card_reports_its_model_and_its_painted_parts(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "openWithState", &[], "{\"opened\":false}");
    update(cx, handle, |shell, _, _| {
        let card = OpenWith::open(1, "photo.png");
        shell.open_with = Some(OpenWithOpen {
            side: 0,
            id: 1,
            card,
            frame: Default::default(),
            initial: Default::default(),
        });
    });
    // The design CI tabs through these controls; every field the reader does not send is named, so a probe that reads one fails on its name.
    let state = json(cx, handle, "openWithState");
    let text = |key| state.get(key).and_then(Json::as_str).map(str::to_owned);
    let model = r#"{"opened":true,"busy":true,"committing":false,"kind":"","mime":"","name":"photo.png","always":false,"error":"","search":"","cursor":0,"rows":[],"controls":[{"name":"Field","focused":true,"enabled":true},{"name":"Applications","focused":false,"enabled":true},{"name":"Always","focused":false,"enabled":true},{"name":"Cancel","focused":false,"enabled":true},{"name":"Open","focused":false,"enabled":false}],"#;
    assert!(read(cx, handle, "openWithState", &[])
        .stdout
        .starts_with(model));
    assert_eq!(
        text("unavailable").as_deref(),
        Some("controls[].visible, controls[].hovered, controls[].pressed, controls[].centre, controls[].rect")
    );
    // No frame has painted the card in this test, so every part reads empty rather than invented.
    for key in [
        "rect",
        "titleRuleRect",
        "searchRect",
        "listRect",
        "rowRect",
        "noMatchRect",
    ] {
        assert_eq!(text(key).as_deref(), Some(""), "{key}");
    }
    for key in ["rowHeight", "eyebrowHeight", "gap", "searchTrail"] {
        assert!(
            state
                .get(key)
                .and_then(Json::as_f64)
                .is_some_and(|token| token > 0.),
            "{key}"
        );
    }
    refused(
        cx,
        handle,
        "menuDialogState",
        "rect, controls, confirmation",
    );
}

#[gpui::test]
fn uish_open_with_card_reports_v0312_model_facts(cx: &mut TestAppContext) {
    let handle = open(cx);
    update(cx, handle, |shell, _, _| {
        let mut card = OpenWith::open(1, "notes.txt");
        let reply = flea::jsondoc::parse(r#"{"t":"menuaction","id":1,"op":"applications","ok":true,"path":"/fx/notes.txt","kind":"Plain text document","mime":"text/plain","applications":[{"id":"nvim.desktop","label":"Neovim","icon":"nvim","default":true}],"installed":[{"id":"xfce4-about.desktop","label":"About Xfce","icon":"","default":false}]}"#).unwrap();
        card.reply(&shell.locale.catalog, &reply);
        shell.open_with = Some(OpenWithOpen {
            side: 0,
            id: 1,
            card,
            frame: Default::default(),
            initial: Default::default(),
        });
    });
    // ui/Ipc.qml openWithState's model fields, in its names and row shapes, before the painted parts.
    let model = r#"{"opened":true,"busy":false,"committing":false,"kind":"Plain text document","mime":"text/plain","name":"notes.txt","always":false,"error":"","search":"","cursor":0,"rows":[{"eyebrow":"Registered for Plain text document","rule":false},{"id":"nvim.desktop","label":"Neovim","icon":"nvim","isDefault":true},{"eyebrow":"All applications","rule":true},{"id":"xfce4-about.desktop","label":"About Xfce","icon":"","isDefault":false}],"controls":[{"name":"Field","focused":true,"enabled":true},{"name":"Applications","focused":false,"enabled":true},{"name":"Always","focused":false,"enabled":true},{"name":"Cancel","focused":false,"enabled":true},{"name":"Open","focused":false,"enabled":true}],"#;
    let state = read(cx, handle, "openWithState", &[]).stdout;
    let state = state.trim_end();
    assert!(state.starts_with(model), "{state}");
    assert!(state.ends_with(r#","unavailable":"controls[].visible, controls[].hovered, controls[].pressed, controls[].centre, controls[].rect"}"#), "{state}");
}

#[gpui::test]
fn ipc_readers3_run_card_reports_its_geometry(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "runCardState", &[], "{\"opened\":false}");
    update(cx, handle, |shell, _, cx| {
        shell.open_run_card(
            0,
            "/files/run.sh".into(),
            r#"{"ask":"run","kind":"script","size":9420,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":11,"mtimeNs":12,"target":"/real/tool.sh"}"#.into(),
            false,
            cx,
        )
    });
    update(cx, handle, |shell, _, _| {
        assert!(shell.run_card.is_some(), "the run card opens");
    });
    // RunCard.qml's runCardState: the ask, its buttons, the default and the painted boxes.
    let state = json(cx, handle, "runCardState");
    let text = |key: &str| state.get(key).and_then(Json::as_str).map(str::to_owned);
    assert_eq!(text("kind").as_deref(), Some("script"));
    assert_eq!(text("focus").as_deref(), Some("terminal"));
    assert_eq!(text("default").as_deref(), Some("terminal"));
    assert!(
        text("rect").is_some_and(|rect| rect.split(' ').count() == 4),
        "{state:?}"
    );
    let controls = state
        .get("controls")
        .and_then(Json::as_array)
        .expect("controls");
    let names: Vec<_> = controls
        .iter()
        .filter_map(|control| control.get("name").and_then(Json::as_str))
        .collect();
    assert_eq!(names, ["cancel", "text", "terminal"]);
    assert!(
        controls.iter().all(|control| control
            .get("centre")
            .and_then(Json::as_str)
            .is_some_and(|centre| centre.split(' ').count() == 2)),
        "{state:?}"
    );
    refused(
        cx,
        handle,
        "menuDialogState",
        "rect, controls, confirmation",
    );
}

#[gpui::test]
fn ipc_readers3_archive_cards_read_closed_then_answer_the_open_card(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "archiveCardsState", &[], "{\"opened\":false}");
    update(cx, handle, |shell, _, _| {
        shell.operations.archive.extract.open(
            "/files/private.7z",
            "/files/out",
            "",
            &shell.locale.catalog,
        );
    });
    // EY-325: the open card answers v0.3.12's shape instead of refusing, so a lane can wait on it.
    let reply = read(cx, handle, "archiveCardsState", &[]);
    assert_eq!(reply.code, 0, "{reply:?}");
    for part in [
        "\"opened\":true",
        "\"compressOpened\":false",
        "\"compressCapture\":null",
        "\"extractOpened\":true",
        "\"extractLeaf\":\"private.7z\"",
        "\"extractFocus\":0",
        "\"canExtract\":false",
    ] {
        assert!(reply.stdout.contains(part), "{part} in {}", reply.stdout);
    }
}

#[gpui::test]
fn ipc_readers3_favourites_saving_reads_idle_at_rest(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "favouritesSaving", &[], "false");
}

#[gpui::test]
fn ipc_readers3_permissions_input_ready_follows_list_focus(cx: &mut TestAppContext) {
    let handle = open(cx);
    let state = json(cx, handle, "permissionsState");
    assert_eq!(state.get("opened").and_then(Json::as_bool), Some(false));
    assert_eq!(
        state.get("inputReady").and_then(Json::as_bool),
        Some(false),
        "an unfocused list takes no input"
    );
    // v0.3.12's listArea.activeFocus needs the active window too; a TestWindow starts inactive.
    update(cx, handle, |shell, window, cx| {
        window.focus(&shell.focus, cx);
        window.activate_window();
    });
    cx.run_until_parked();
    let state = json(cx, handle, "permissionsState");
    assert_eq!(
        state.get("inputReady").and_then(Json::as_bool),
        Some(true),
        "the focused list with no listing in flight takes input"
    );
}

#[gpui::test]
fn ipc_readers3_context_menu_focus_reports_an_open_menu(cx: &mut TestAppContext) {
    let handle = open(cx);
    let state = json(cx, handle, "contextMenuFocusState");
    assert_eq!(state.get("opened").and_then(Json::as_bool), Some(false));
    update(cx, handle, |shell, window, cx| {
        shell.open_menu(0, None, (40., 40.), window, cx)
    });
    update(cx, handle, |shell, _, _| {
        assert!(shell.menu.menu.borrow().opened(), "the pane opened a menu");
    });
    // EY-228: the open menu answers v0.3.12's JSON; the menu holds the keys, so the opener (origin) does not.
    let state = json(cx, handle, "contextMenuFocusState");
    assert_eq!(state.get("opened").and_then(Json::as_bool), Some(true));
    assert_eq!(state.get("menu").and_then(Json::as_bool), Some(true));
    assert_eq!(state.get("origin").and_then(Json::as_bool), Some(false));
    assert_eq!(state.get("activeItem").and_then(Json::as_str), Some(""));
}

#[gpui::test]
fn ipc_readers3_glass_reads_once_the_window_is_mounted(cx: &mut TestAppContext) {
    let handle = open(cx);
    assert_eq!(read(cx, handle, "glass", &[]).code, 3);
    update(cx, handle, |_, window, cx| {
        crate::window::glass::mount(&*window, WindowBackgroundAppearance::Opaque, cx)
    });
    let state = json(cx, handle, "glass");
    assert_eq!(state.get("clear").and_then(Json::as_bool), Some(false));
    assert_eq!(state.get("listAlpha").and_then(Json::as_f64), Some(1.));
    assert!(state.get("window").and_then(Json::as_str).is_some());
}

#[gpui::test]
fn ipc_readers3_share_browser_is_idle_and_reports_pane_list_areas(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "shareBrowserRect", &[], "");
    let state = json(cx, handle, "shareBrowserState");
    assert_eq!(state.get("active").and_then(Json::as_bool), Some(false));
    assert_eq!(state.get("owner").and_then(Json::as_f64), Some(-1.));
    assert_eq!(state.get("rect").and_then(Json::as_str), Some(""));
    assert_eq!(state.get("baseUri").and_then(Json::as_str), Some(""));
    assert_eq!(state.get("cursor").and_then(Json::as_f64), Some(-1.));
    assert!(matches!(state.get("paneRects"), Some(Json::Arr(_))));
}

#[gpui::test]
fn ipc_readers3_row_thumb_ready_is_false_without_a_row(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "rowThumbReady", &["0"], "false");
}

#[gpui::test]
fn ipc_readers3_listing_drop_is_inactive_without_a_drag(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "listingDropActive", &[], "false");
}

#[gpui::test]
fn ipc_readers3_rail_mark_is_never_hovered_by_idle_gpui(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "railMarkHovered", &["0"], "false");
}

#[gpui::test]
fn ipc_readers3_context_menu_focus_closed_shape(cx: &mut TestAppContext) {
    let handle = open(cx);
    let state = json(cx, handle, "contextMenuFocusState");
    assert_eq!(state.get("opened").and_then(Json::as_bool), Some(false));
    assert_eq!(state.get("menu").and_then(Json::as_bool), Some(false));
    assert_eq!(state.get("origin").and_then(Json::as_bool), Some(false));
    assert_eq!(state.get("activeItem").and_then(Json::as_str), Some(""));
    for key in ["view", "pane", "list"] {
        assert!(
            state.get(key).is_some(),
            "{key} is part of the v0.3.12 shape"
        );
    }
}

#[gpui::test]
fn ipc_readers3_thumbnail_policy_idle_shape(cx: &mut TestAppContext) {
    let handle = open(cx);
    let state = json(cx, handle, "thumbnailPolicyState");
    for key in [
        "view",
        "mode",
        "files",
        "pending",
        "contentY",
        "cursor",
        "previewIndex",
        "previewPath",
        "previewReady",
    ] {
        assert!(
            state.get(key).is_some(),
            "{key} is part of the v0.3.12 shape"
        );
    }
    assert_eq!(state.get("previewIndex").and_then(Json::as_f64), Some(-1.));
    assert_eq!(
        state.get("previewReady").and_then(Json::as_bool),
        Some(false)
    );
}
