use super::*;

#[path = "status_notice_layout_tests.rs"]
mod layout;

// v0.4.0's shipped Menus switches, which hide all three rows a locked folder can offer (locked-menu board).
pub(super) const HIDDEN_0_4_0: &str = r#"["delete","openTerminal","placeMenu","runScript","moveto","copyto","properties","permissions","copyAs","pasteAs","invertSelection","extThumbs","run","openAsText","openAgent"]"#;

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
    let handle = locked(cx, Some(HIDDEN_0_4_0));
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

// Permissions ships shown, so at the shipped defaults a locked folder's menu opens on that row alone, and 0.4.0's notice still names what stays hidden.
#[gpui::test]
fn ey48_locked_folder_at_the_shipped_defaults_offers_permissions_and_names_the_hidden_two(
    cx: &mut TestAppContext,
) {
    let handle = locked(cx, None);
    handle
        .update(cx, |shell, _, _| {
            assert!(shell.menu_opened(), "the locked menu opens");
            let ids: Vec<String> = shell
                .menu_entries()
                .iter()
                .filter(|e| !e.separator)
                .map(|e| e.action.to_string())
                .collect();
            assert_eq!(ids, ["permissions"]);
        })
        .unwrap();
    assert_eq!(
        read(cx, handle, "lastMessage"),
        Reply::value("Open in Terminal and Copy As are hidden in Settings > Menus.")
    );
    assert_eq!(read(cx, handle, "statusError"), Reply::value("false"));
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
fn record(argv: &[String]) -> crate::g2::open_with_properties::lane::Exit {
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
                crate::g2::context_menus::menu::Outcome::Fire {
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

#[gpui::test]
fn ey48_keyboard_sheet_finds_the_locked_menus_rows_even_when_menus_hides_them(
    cx: &mut TestAppContext,
) {
    // The sheet reads the menu the key would open, hidden rows included, so a locked folder lists its own rows.
    let handle = locked(cx, Some(r#"["permissions"]"#));
    handle
        .update(cx, |shell, _, _| {
            let sheet: Vec<String> = shell
                .sheet_menus()
                .into_iter()
                .map(|entry| entry.action)
                .collect();
            let mode = shell.panes[0].as_ref().unwrap().pane.state.mode;
            let rows = |hidden: &[String]| -> Vec<String> {
                flea::ui_logic_context_menus::places::locked_entries(mode, hidden)
                    .into_iter()
                    .filter(|entry| !entry.separator)
                    .map(|entry| entry.action.to_string())
                    .collect()
            };
            let menu = rows(&shell.menu_hidden());
            assert_eq!(menu.len(), 2, "the menu hides Permissions: {menu:?}");
            let locked = rows(&[]);
            assert_eq!(locked.len(), 3, "{locked:?}");
            let mut sheet_rows = sheet.clone();
            // sheet_entries adds a dead Paste As while nothing is held, a row no locked menu carries.
            sheet_rows.retain(|action| action != "pasteAs");
            assert_eq!(
                sheet_rows, locked,
                "the sheet lists the locked rows: {sheet:?}"
            );
        })
        .unwrap();
}

// Census w2.locked.target-copy: no shipped Locked row reaches Pane.qml performLocked's fallback, so a row outside the whitelist drives it.
#[gpui::test]
fn ruling34_a_locked_tile_row_nobody_runs_says_the_action_is_not_available_yet(
    cx: &mut TestAppContext,
) {
    let handle = locked(cx, Some("[]"));
    handle
        .update(cx, |shell, window, cx| {
            assert!(shell.menu_opened(), "the locked menu opens");
            let action = "outsideLockedWhitelist";
            assert!(
                !shell
                    .menu_entries()
                    .iter()
                    .any(|entry| entry.action == action),
                "the row is outside the Locked whitelist"
            );
            shell.menu.pending = Some((action.into(), String::new()));
            shell.deliver_menu(vec!["/files".into()], window, cx);
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(
        read(cx, handle, "lastMessage"),
        Reply::value("That action is not available yet")
    );
    assert_eq!(
        read(cx, handle, "statusError"),
        Reply::value("false"),
        "Pane.qml says it as a plain message"
    );
}

#[path = "fixlogic2_locked_menu_tests.rs"]
mod fixlogic2_locked;

// EY-496: m over the Locked hero opens its menu at the middle of the list slot, as Pane.qml openCursorMenu anchors it.
#[gpui::test]
fn fixmenus_ey496_the_locked_menu_opens_at_its_hero_not_the_first_row(cx: &mut TestAppContext) {
    let handle = locked(cx, Some("[]"));
    handle
        .update(cx, |shell, window, _| {
            assert!(shell.menu_opened(), "the locked menu opens");
            let pane = shell
                .geometry(window)
                .rows(0, false, shell.slots.strip_height);
            assert_eq!(
                shell.menu.menu.borrow().anchor(),
                (pane.x + pane.width / 2., pane.y + pane.height / 2.)
            );
        })
        .unwrap();
}
