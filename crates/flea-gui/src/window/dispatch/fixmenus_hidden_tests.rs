//! FIX-MENUS EY-385 through the mounted window: Show Hidden Files from the background menu runs the . key's settings write.
use super::{
    menus_dual_tests::spot,
    menus_tests::{actions, choose, entry, right_click, settle},
    test_shell::{asked_on, land, land_on, open_root, sent, set_cursor},
};
use crate::{
    state::Shell,
    window::{navigate::Step, startup::View, u07_tests::read},
};
use flea::ui_logic_settings_pages::pages;
use gpui::{Entity, TestAppContext, VisualTestContext};

const FILE: u32 = 0o100644;

#[gpui::test]
fn fixmenus_ey385_background_show_hidden_relists_fresh_as_the_key_does(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let rows = (0..4)
        .map(|at| entry(&format!("file-{at}.txt"), FILE))
        .collect();
    land(&shell, cx, "/files", rows, 1);
    set_cursor(&shell, cx, 2);
    right_click(cx, 600., 350.);
    assert!(shell.read_with(cx, |shell, _| shell
        .menu
        .captured
        .as_ref()
        .is_some_and(|capture| capture.background)));
    sent();
    choose(&shell, cx, "toggleHidden");
    let lists: Vec<_> = sent()
        .into_iter()
        .filter(|line| line.contains(r#""c":"list""#))
        .collect();
    assert!(
        lists.len() == 1 && lists[0].contains(r#""hidden":true"#),
        "the menu re-lists the folder with hidden files shown: {lists:?}"
    );
    shell.read_with(cx, |shell, _| {
        assert!(pages::is_true(&shell.launch.settings.data, &["hidden"]));
        let host = shell.panes[0].as_ref().unwrap();
        // v0.3.12 Pane.qml toggleHidden re-opens the folder, which clears the cursor, as the . key's settings relist does.
        assert!(
            host.nav.anchor.is_none(),
            "the relist is a fresh listing, never anchored on the cursor row"
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn fixmenus_ey385_show_hidden_chosen_during_a_reread_is_refused_and_lists_nothing(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open_root(cx, "default");
    let rows = || vec![entry("a.txt", FILE), entry("b.txt", FILE)];
    land(&shell, cx, "/files", rows(), 1);
    right_click(cx, 600., 350.);
    // A watcher's reread goes out while the menu is up, as a change on disk starts one.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.step(0, Step::Refresh(None), window, cx), Ok(true))
        })
    });
    sent();
    choose(&shell, cx, "toggleHidden");
    let hidden = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| {
            pages::is_true(&shell.launch.settings.data, &["hidden"])
        })
    };
    assert!(
        !hidden(&shell, cx),
        "a menu whose listing moved under it changes no setting"
    );
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        shell.read_with(cx, |shell, _| shell.locale.catalog.tr("menu.refusal.moved"))
    );
    land(&shell, cx, "/files", rows(), 2);
    let lists: Vec<_> = sent()
        .into_iter()
        .filter(|line| line.contains(r#""c":"list""#))
        .collect();
    assert!(
        lists.is_empty() && !hidden(&shell, cx),
        "so no setting and listing can disagree once the reread lands: {lists:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// The other order: a reread already out when the menu opens leaves the pane busy, and a busy pane raises no rows.
#[gpui::test]
fn fixmenus_ey385_a_menu_raised_during_a_reread_offers_no_show_hidden(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let rows = || vec![entry("a.txt", FILE), entry("b.txt", FILE)];
    land(&shell, cx, "/files", rows(), 1);
    right_click(cx, 600., 350.);
    assert!(
        actions(&shell, cx).iter().any(|row| row == "toggleHidden"),
        "the same click on the settled pane offers it"
    );
    cx.simulate_keystrokes("escape");
    settle(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.step(0, Step::Refresh(None), window, cx), Ok(true))
        })
    });
    sent();
    assert!(shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.busy()));
    right_click(cx, 600., 350.);
    assert_eq!(
        actions(&shell, cx),
        Vec::<String>::new(),
        "the busy gate raises no rows, so Show Hidden Files is never offered over a listing still being read"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn fixmenus_ey385_show_hidden_from_a_recent_panes_own_menu_is_refused(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![entry("a.txt", FILE)], 1);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().recent.on = true
    });
    settle(cx);
    right_click(cx, 600., 350.);
    choose(&shell, cx, "toggleHidden");
    assert!(
        !shell.read_with(cx, |shell, _| pages::is_true(
            &shell.launch.settings.data,
            &["hidden"]
        )),
        "Pane.qml:553: Recent shows every item it lists, so its own menu changes nothing"
    );
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        shell.read_with(cx, |shell, _| shell
            .locale
            .catalog
            .tr("places.recent.hidden_refused"))
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn fixmenus_ey385_show_hidden_from_the_unfocused_pane_obeys_that_pane(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let rows = || vec![entry("a.txt", FILE), entry("b.txt", FILE)];
    land(&shell, cx, "/files", rows(), 1);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Dual, window, cx)));
    let right = asked_on(&shell, cx, 1);
    land_on(&shell, cx, 1, &right, rows(), 2, 1);
    // The focused left pane stands in Recent, which refuses the hidden toggle for itself alone.
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().recent.on = true
    });
    settle(cx);
    let (x, y) = spot(&shell, cx, 1, None);
    right_click(cx, x, y);
    assert_eq!(
        shell.read_with(cx, |shell, _| (
            shell.focus_side,
            shell.menu.captured.as_ref().map(|c| (c.side, c.background))
        )),
        (0, Some((1, true)))
    );
    choose(&shell, cx, "toggleHidden");
    assert!(
        shell.read_with(cx, |shell, _| pages::is_true(
            &shell.launch.settings.data,
            &["hidden"]
        )),
        "the right pane's menu toggles, never refused by the focused Recent pane"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
