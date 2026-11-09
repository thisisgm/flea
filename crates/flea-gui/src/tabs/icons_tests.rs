//! 0401-tabs-icons-a: Show tabs as icons narrows every tab to its folder's mark, drops the close slot and keeps every action.
use super::bar::{Hit, PAD_X};
use super::icons::{mark_for, resting_tab, REST_MS, WIDTH};
use super::join_tests::{key, read};
use super::Bar;
use crate::{backend::Backend, ipc::Adapter, state::tests::shell_with};
use flea::ipc::wire::Reply;
use flea::ui_logic_rail_places::rail::RailEntry;
use gpui::{point, px, AppContext, TestAppContext};

fn place(path: &str, glyph: &'static str) -> RailEntry {
    RailEntry {
        path: path.into(),
        glyph,
        ..RailEntry::default()
    }
}

#[test]
fn each_tab_takes_the_mark_its_folder_draws_in_the_rail() {
    let rail = [
        place("/home/gm", "house"),
        place("/mnt/nas", "server"),
        place("/run/media/gm/QEMU", "drive"),
        place("", "network"),
    ];
    let marks: Vec<&str> = [
        "/home/gm",
        "/mnt/nas",
        "/run/media/gm/QEMU",
        "/run/media/gm/QEMU/docs",
        "/fx/v/work",
        "",
    ]
    .iter()
    .map(|path| mark_for(path, "/home/gm", &rail))
    .collect();
    assert_eq!(
        marks,
        ["house", "server", "drive", "folder", "folder", "folder"]
    );
    assert_eq!(
        mark_for("/home/gm", "/home/gm", &[]),
        "house",
        "home draws the house with the rail's Home row hidden"
    );
}

#[test]
fn an_icon_tab_is_44_wide_and_has_no_close_slot() {
    let mut bar = Bar::new("test");
    bar.width = 1100.;
    let named = bar.tab_width(3);
    let close = PAD_X + named - 4.;
    assert_eq!(bar.hit(close, 3), Hit::Close(0));
    bar.icons = true;
    assert_eq!(bar.tab_width(3), WIDTH);
    for x in [PAD_X + 1., PAD_X + WIDTH - 1.] {
        assert_eq!(bar.hit(x, 3), Hit::Tab(0));
    }
    assert_eq!(bar.hit(PAD_X + WIDTH + 2., 3), Hit::Tab(1));
    assert_eq!(bar.hit(PAD_X + 3. * WIDTH + 2., 3), Hit::New);
    assert_eq!(
        bar.menu_at(PAD_X + WIDTH - 1., 3).map(|ask| ask.index),
        Some(0),
        "the tab menu still opens"
    );
}

#[gpui::test]
fn the_setting_reaches_the_drawn_strip_and_off_restores_named_tabs(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            key(shell, "t", window, cx);
            shell.sync_tabs(window, cx);
            let named = shell.strip.snapshot.as_ref().unwrap().frame.clone();
            assert!(named.icons.is_none() && named.tab_width > WIDTH);
            shell.launch.settings.data = flea::jsondoc::parse(r#"{"tabsAsIcons":true}"#).unwrap();
            shell.sync_tabs(window, cx);
            let frame = shell.strip.snapshot.as_ref().unwrap().frame.clone();
            let icons = frame.icons.as_ref().expect("the frame carries marks");
            assert_eq!(
                (frame.tab_width, icons.marks.len(), icons.paths.len()),
                (WIDTH, 3, 3)
            );
            assert_eq!(
                icons.paths,
                frame
                    .labels
                    .iter()
                    .map(|_| icons.paths[0].clone())
                    .collect::<Vec<_>>(),
                "t opens the current folder"
            );
            // The close slot's place on a named tab selects the tab instead.
            let hit = shell.tab_hit(point(px(PAD_X + WIDTH - 4.), px(12.)));
            assert_eq!(hit, Hit::Tab(0));
            // tabCentre and the drawn strip both read this width; the native run clicks through tabCentre.
            assert_eq!(shell.strip.bar.tab_width(3), WIDTH);
            assert_eq!(read(shell, "tabCount", window, cx), Reply::value("3"));
            shell.launch.settings.data = flea::jsondoc::parse("{}").unwrap();
            shell.sync_tabs(window, cx);
            let back = shell.strip.snapshot.as_ref().unwrap().frame.clone();
            assert_eq!(back, named, "off draws the named strip again");
            assert!(!shell.strip.bar.icons);
        });
    });
    cx.run_until_parked();
}

#[gpui::test]
fn the_tip_shows_only_after_the_rest_and_clears_when_the_pointer_leaves(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let (_shell, strip) = cx.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        let strip = entity.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            shell.launch.settings.data = flea::jsondoc::parse(r#"{"tabsAsIcons":true}"#).unwrap();
            shell.sync_tabs(window, cx);
            shell.strip.view.clone().expect("the strip view")
        });
        (entity, strip)
    });
    let wait = |cx: &mut gpui::VisualTestContext, ms: u64| {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(ms));
        cx.run_until_parked();
    };
    let shown =
        |cx: &mut gpui::VisualTestContext| strip.read_with(cx, |strip, _| strip.tip.shown());
    strip.update(cx, |strip, cx| strip.rest_on(1, cx));
    wait(cx, REST_MS - 1);
    assert_eq!(shown(cx), None, "no tip before the rest has passed");
    wait(cx, 1);
    assert_eq!(shown(cx), Some(1));
    strip.update(cx, |strip, cx| strip.rest_on(0, cx));
    assert_eq!(shown(cx), None, "another tab starts the rest over");
    wait(cx, REST_MS / 2);
    strip.update(cx, |strip, cx| strip.tip_left(cx));
    wait(cx, REST_MS);
    assert_eq!(shown(cx), None, "leaving mid-rest drops the pending tip");
    strip.update(cx, |strip, cx| strip.rest_on(0, cx));
    wait(cx, REST_MS / 2);
    strip.update(cx, |strip, cx| strip.rest_on(1, cx));
    wait(cx, REST_MS / 2);
    assert_eq!(shown(cx), None, "tab 0's pending rest never lands on tab 1");
    wait(cx, REST_MS - REST_MS / 2);
    assert_eq!(shown(cx), Some(1));
    strip.update(cx, |strip, cx| strip.tip_left(cx));
    assert_eq!(shown(cx), None);
}

#[test]
fn a_pointer_rests_only_on_a_tab_of_marks_with_nothing_pressed_held_or_dragged() {
    let tab = || Hit::Tab(2);
    assert_eq!(resting_tab(true, false, false, false, tab), Some(2));
    assert_eq!(
        resting_tab(false, false, false, false, tab),
        None,
        "named tabs keep no tip"
    );
    assert_eq!(
        resting_tab(true, true, false, false, tab),
        None,
        "a button is down"
    );
    assert_eq!(
        resting_tab(true, false, true, false, tab),
        None,
        "a tab switch is held"
    );
    assert_eq!(
        resting_tab(true, false, false, true, tab),
        None,
        "a tab is dragged"
    );
    for hit in [Hit::New, Hit::Nothing, Hit::Close(0)] {
        assert_eq!(resting_tab(true, false, false, false, || hit), None);
    }
}

#[gpui::test]
fn a_shown_tip_leaves_the_strip_bounds_its_hits_and_tab_centre_read(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            key(shell, "t", window, cx);
            shell.launch.settings.data = flea::jsondoc::parse(r#"{"tabsAsIcons":true}"#).unwrap();
            shell.sync_tabs(window, cx);
        })
    });
    cx.run_until_parked();
    // The window point at tab 2's middle, as the strip is first drawn.
    let at = cx.update(|_, cx| {
        let bounds = shell
            .read(cx)
            .strip
            .bounds
            .get()
            .expect("the strip is drawn");
        point(
            bounds.origin.x + px(PAD_X + 2.5 * WIDTH),
            bounds.origin.y + px(12.),
        )
    });
    let drawn = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| {
            let shell = shell.read(cx);
            let home = shell.locale.catalog.tr("tabs.home");
            let centre = shell.tab_adapter(&home).call("tabCentre", &["2".into()]);
            (shell.strip.bounds.get(), shell.tab_hit(at), centre)
        })
    };
    let before = drawn(cx);
    assert_eq!(before.1, Hit::Tab(2));
    let strip = cx.update(|_, cx| shell.read(cx).strip.view.clone().expect("the strip view"));
    strip.update(cx, |strip, cx| strip.rest_on(1, cx));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(REST_MS));
    cx.run_until_parked();
    assert_eq!(strip.read_with(cx, |strip, _| strip.tip.shown()), Some(1));
    assert_eq!(
        drawn(cx),
        before,
        "the tip is drawn over the panes, not taken for the strip"
    );
}
