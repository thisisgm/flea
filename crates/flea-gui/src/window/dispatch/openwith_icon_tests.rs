//! EY-463: each Open With flyout row draws its application's own icon, as file-menu.png draws Neovim's.
use super::menudeliver_tests::{open, select, wire, REG};
use super::menus_tests::{choose, deliver, home, reply, settle};
use super::test_shell::land;
use crate::{g2::context_menus::app_icons, state::Shell};
use flea::ui_logic_context_menus::fit;
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::collections::BTreeSet;

const ICON: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/images/icon.svg");
// A bare Icon= name takes the themed lookup, so the unplaced fixture names nothing any theme ships.
const UNPLACED: &str = "flea-fixture-unplaced-application";

fn applications(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    apps: &str,
) -> impl FnMut() -> Vec<String> {
    land(shell, cx, "/files", home(), 1);
    select(shell, cx, &[2]);
    let sent = wire(shell, cx);
    cx.simulate_keystrokes("shift-f10");
    let id = shell.read_with(cx, |shell, _| shell.menu.captured.as_ref().unwrap().id);
    deliver(
        shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[{REG}]}}"#),
    );
    reply(
        shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[{apps}]}}"#
        ),
    );
    sent
}

fn app(icon: &str, at: usize) -> String {
    format!(
        r#"{{"id":"app{at}.desktop","label":"App {at:03}","icon":"{}","default":{}}}"#,
        flea::json::escape(icon),
        at == 0
    )
}

/// The visible rows are independently observed through the menu's pointer hit testing.
fn visible_icons(shell: &Entity<Shell>, cx: &VisualTestContext) -> BTreeSet<String> {
    shell.read_with(cx, |shell, _| {
        let menu = shell.menu.menu.borrow();
        let card = &menu.flyout.as_ref().expect("the flyout is open").card;
        let x = card.frame.x + card.frame.w / 2.;
        (0..card.view_h)
            .filter_map(|pixel| card.row_at(x, card.frame.y + (fit::PAD + pixel) as f32 + 0.5))
            .filter_map(|at| card.entries[at].icon.clone())
            .collect()
    })
}

fn requested_icons(paths: &[String], cx: &mut VisualTestContext) -> BTreeSet<String> {
    cx.update(|window, cx| {
        paths
            .iter()
            .filter(|path| app_icons::requested(path, window, cx))
            .cloned()
            .collect()
    })
}

#[gpui::test]
fn the_flyout_holds_each_applications_own_icon_at_the_mark_size(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    let _sent = applications(
        &shell,
        cx,
        &format!("{},{}", app(ICON, 0), app(UNPLACED, 1)),
    );
    assert!(
        cx.update(|window, cx| !app_icons::requested(ICON, window, cx)),
        "a closed flyout starts no icon work"
    );
    choose(&shell, cx, "openWith");
    settle(cx);
    let (own, unplaced, side) = cx.update(|window, cx| {
        (
            app_icons::held(ICON, window, cx),
            app_icons::held(UNPLACED, window, cx),
            (flea::ui_logic_context_menus::fit::SLOT * window.scale_factor()).round() as i32,
        )
    });
    let own = own.expect("the application's own icon is rasterized for its row");
    let size = own.size(0);
    assert_eq!((size.width.0, size.height.0), (side, side));
    assert!(
        unplaced.is_none(),
        "a name no icon theme places keeps the window mark"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn only_visible_application_rows_start_icon_work(cx: &mut TestAppContext) {
    let paths: Vec<String> = (0..100)
        .map(|at| {
            format!(
                "{}/{}icon.svg",
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/images"),
                "./".repeat(at)
            )
        })
        .collect();
    let apps = paths
        .iter()
        .enumerate()
        .map(|(at, path)| app(path, at))
        .collect::<Vec<_>>()
        .join(",");
    let (shell, cx) = open(cx);
    let _sent = applications(&shell, cx, &apps);
    assert!(requested_icons(&paths, cx).is_empty());
    choose(&shell, cx, "openWith");
    settle(cx);
    let mut expected = visible_icons(&shell, cx);
    assert!(!expected.is_empty() && expected.len() < paths.len());
    assert_eq!(
        requested_icons(&paths, cx),
        expected,
        "only hit-tested application rows start work"
    );
    let before = shell.read_with(cx, |shell, _| {
        shell
            .menu
            .menu
            .borrow()
            .flyout
            .as_ref()
            .unwrap()
            .card
            .content_y
    });
    let wheel_units = -((paths.len() / 2) as i32) * fit::NOTCH_UNITS;
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let outcome = {
                let mut menu = shell.menu.menu.borrow_mut();
                let card = &menu.flyout.as_ref().unwrap().card;
                let x = card.frame.x + card.frame.w / 2.;
                let y = card.frame.y + fit::PAD as f32 + fit::ROW as f32 / 2.;
                menu.wheel(x, y, wheel_units)
            };
            shell.menu_outcome(outcome, window, cx);
        });
    });
    settle(cx);
    assert!(shell.read_with(cx, |shell, _| {
        shell
            .menu
            .menu
            .borrow()
            .flyout
            .as_ref()
            .unwrap()
            .card
            .content_y
            > before
    }));
    expected.extend(visible_icons(&shell, cx));
    assert_eq!(
        requested_icons(&paths, cx),
        expected,
        "scrolling starts only newly visible application rows"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn closing_the_initiating_shell_does_not_strand_a_shared_icon(cx: &mut TestAppContext) {
    let release = cx.update(app_icons::hold_completions);
    let weak = {
        let (shell, visual) = open(cx);
        let _sent = applications(&shell, visual, &app(ICON, 0));
        choose(&shell, visual, "openWith");
        let weak = shell.downgrade();
        visual.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let list = shell.panes[0].as_ref().unwrap().list.borrow();
                let outcome = shell.menu.menu.borrow_mut().act(
                    "menuRight",
                    None,
                    &shell.menu_identity(),
                    &shell.menu_env(&list.typeface),
                    window,
                );
                drop(list);
                shell.menu_outcome(outcome, window, cx);
                shell.load_app_icons(window, cx);
                shell.stop_for_app_quit();
            });
            window.remove_window();
        });
        drop(shell);
        weak
    };
    cx.update(|_| ());
    assert!(
        weak.upgrade().is_none(),
        "the initiating Shell is gone before the raster completes"
    );
    release.close();
    cx.run_until_parked();
    let (shell, visual) = open(cx);
    assert!(
        visual.update(|window, cx| app_icons::held(ICON, window, cx).is_some()),
        "another window can use the completed process-global icon"
    );
    shell.update(visual, |shell, _| shell.stop_for_app_quit());
}
