use super::*;

// EY-48: m over a folder Flea cannot read, the Locked state's pane.
fn locked(cx: &mut TestAppContext, hidden: Option<&str>) -> WindowHandle<Shell> {
    locked_at(cx, hidden, "/files")
}

fn locked_at(cx: &mut TestAppContext, hidden: Option<&str>, folder: &str) -> WindowHandle<Shell> {
    let handle = open(cx);
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            if let Some(hidden) = hidden {
                shell.launch.settings.data =
                    flea::jsondoc::parse(&format!(r#"{{"menu":{{"hidden":{hidden}}}}}"#)).unwrap();
            }
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            assert!(pane.begin(folder.into(), Default::default(), pane.now()));
            shell.receive(
                0,
                crate::backend::Event::Refused {
                    operation: "scan".into(),
                    path: folder.into(),
                    message: "permission denied".into(),
                    mode: 0o040000,
                },
                window,
                cx,
            );
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let state = &shell.panes[0].as_ref().unwrap().pane.state;
            assert_eq!(
                state.locked_target(),
                Some(folder),
                "the pane draws the Locked state"
            );
        })
        .unwrap();
    handle
        .update(cx, |shell, window, cx| shell.menu_keyboard_open(window, cx))
        .unwrap();
    frame(cx, handle);
    handle
}

#[gpui::test]
fn ey48_locked_folder_with_its_rows_hidden_opens_no_menu_and_says_why(cx: &mut TestAppContext) {
    // The shipped Menus switches hide all three rows a locked folder can offer (locked-menu board).
    let handle = locked(cx, None);
    handle
        .update(cx, |shell, _, _| {
            assert!(!shell.menu_opened(), "no menu opens over the locked hero");
            assert!(
                shell.menu.captured.is_none(),
                "no background capture stays armed"
            );
        })
        .unwrap();
    assert_eq!(
        read(cx, handle, "lastMessage"),
        Reply::value("Open in Terminal, Permissions and Copy As are hidden in Settings > Menus.")
    );
    assert_eq!(
        read(cx, handle, "statusError"),
        Reply::value("false"),
        "one foreground sentence, since nothing failed"
    );
    let lane = handle
        .update(cx, |shell, _, cx| {
            let catalog = shell.locale.catalog.clone();
            shell.status_messages(cx, |messages, _| messages.line(&catalog, "").hint)
        })
        .unwrap();
    assert!(
        lane.contains("esc dismisses"),
        "esc dismisses follows it, muted: {lane}"
    );
}

#[gpui::test]
fn ey48_locked_folder_offers_only_the_rows_that_act_on_it(cx: &mut TestAppContext) {
    let handle = locked(cx, Some("[]"));
    handle
        .update(cx, |shell, _, _| {
            let ids: Vec<String> = shell
                .menu_entries()
                .iter()
                .filter(|e| !e.separator)
                .map(|e| e.action.to_string())
                .collect();
            assert_eq!(
                ids,
                ["openTerminal", "permissions", "copypath"],
                "LockedMenu.js lockedEntries, never New Folder or Paste"
            );
        })
        .unwrap();
}

static LAUNCHED: std::sync::Mutex<Vec<Vec<String>>> = std::sync::Mutex::new(Vec::new());

// Stands in for the opener lane's process launch and records each argv.
fn record(argv: &[String]) -> crate::g2::op3::lane::Exit {
    LAUNCHED.lock().unwrap().push(argv.to_vec());
    (Some(0), String::new())
}

#[gpui::test]
fn ey48_a_locked_child_folder_is_what_its_rows_act_on(cx: &mut TestAppContext) {
    let handle = locked_at(cx, Some("[]"), "/files/secret");
    handle
        .update(cx, |shell, window, cx| {
            assert_eq!(
                shell.panes[0].as_ref().unwrap().path,
                "/files",
                "the pane keeps its own path"
            );
            assert!(shell.menu_opened(), "the locked menu opens");
            shell.opener[0].launch = record;
            shell.menu_outcome(
                crate::g2::mn1::menu::Outcome::Fire {
                    action: "openTerminal".into(),
                    leaf: String::new(),
                },
                window,
                cx,
            );
        })
        .unwrap();
    frame(cx, handle);
    let launched = LAUNCHED.lock().unwrap().clone();
    assert_eq!(launched.len(), 1, "one terminal launch: {launched:?}");
    assert_eq!(
        launched[0][1..],
        ["--terminal", "/files/secret"],
        "the denied folder, not the pane's"
    );
}
