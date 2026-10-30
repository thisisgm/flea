//! The Favorites save reader follows writes chosen through a mounted native window's menu.
use super::store::{Update, Worker};
use crate::{
    backend::{Backend, Event},
    state::Shell,
    window::{
        locale::Snapshot,
        root::Root,
        startup::{Arguments, Launch, Settings},
        u07_tests::read,
    },
};
use flea::{jsondoc, jsondoc::Json, protocol::Reply};
use gpui::{
    point, px, size, AppContext, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext,
};
use std::{
    sync::{mpsc, Arc},
    time::Duration,
};

type Write = (u64, Json, Option<Json>);

fn open(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, mpsc::Receiver<Write>) {
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, _| Ok(Backend::test_channel().0),
            monitor: None,
        });
    });
    let settings = Settings::from_json(flea::uistate::from_file(
        r#"{"view":"list","places":{"rail":"shown"}}"#,
    ));
    let launch = Launch::resolve(
        settings,
        Arguments {
            path: Some("/files".into()),
            select: None,
        },
        "/",
    );
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let (_, events) = async_channel::unbounded();
    let (root, cx) = cx.add_window_view(|window, cx| Root {
        shell: cx.new(|cx| Shell::new(launch, locale, events, window, cx)),
    });
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    let (writes, receive) = mpsc::channel();
    let (_, updates) = async_channel::unbounded();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.settings.worker = Some(Worker { writes, updates });
            *shell.panes[0].as_ref().unwrap().nav.writer.borrow_mut() = Default::default();
            window.focus(&shell.focus, cx);
            shell.receive(
                0,
                Event::Reply(Reply::Listed {
                    path: "/files".into(),
                    total: 0,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                Event::Reply(Reply::Rows {
                    start: 0,
                    rows: Vec::new(),
                    listing: 1,
                }),
                window,
                cx,
            );
        })
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    settle(cx);
    (shell, cx, receive)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
}

fn add_from_background(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    cx.simulate_event(gpui::MouseDownEvent {
        position: point(px(600.), px(500.)),
        button: MouseButton::Right,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_mouse_up(
        point(px(600.), px(500.)),
        MouseButton::Right,
        Modifiers::none(),
    );
    settle(cx);
    let rows = jsondoc::parse(&read(shell, cx, "contextMenuModel", &[])).unwrap();
    let index = rows
        .as_array()
        .unwrap()
        .iter()
        .position(|row| row.get("action").and_then(Json::as_str) == Some("addFavourite"))
        .expect("background offers Add to Favorites");
    for _ in 0..64 {
        let cursor: usize = read(shell, cx, "contextMenuCursor", &[]).parse().unwrap();
        if cursor == index {
            cx.simulate_keystrokes("enter");
            settle(cx);
            return;
        }
        cx.simulate_keystrokes(if cursor < index { "down" } else { "up" });
    }
    panic!("the menu cursor never reached Add to Favorites");
}

#[gpui::test]
fn favourites_saving_is_mounted_on_a_normal_root(cx: &mut TestAppContext) {
    let (shell, cx, _) = open(cx);
    assert_eq!(read(&shell, cx, "favouritesSaving", &[]), "false");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn favourites_saving_tracks_a_walked_write_until_its_answer(cx: &mut TestAppContext) {
    let (shell, cx, writes) = open(cx);
    add_from_background(&shell, cx);
    let (revision, _, operation) = writes.try_recv().expect("the menu queued a write");
    assert_eq!(
        operation.unwrap().get("op").and_then(Json::as_str),
        Some("add")
    );
    assert_eq!(read(&shell, cx, "favouritesSaving", &[]), "true");
    cx.update(|window, cx| shell.update(cx, |shell, cx| {
        shell.change_settings(jsondoc::parse(r#"{"hidden":true}"#).unwrap(), window, cx);
        shell.receive_settings(Update::Loaded {
            document: flea::uistate::from_file(r#"{"view":"list","places":{"favourites":[{"path":"/files","label":"files"}]}}"#),
            through: revision, saved: false,
        }, window, cx);
    }));
    assert!(
        writes.try_recv().unwrap().2.is_none(),
        "an unrelated setting is still pending"
    );
    assert_eq!(read(&shell, cx, "favouritesSaving", &[]), "false");
    assert!(read(&shell, cx, "uiSettings", &[]).contains("/files"));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn favourites_saving_clears_after_a_failed_write(cx: &mut TestAppContext) {
    let (shell, cx, writes) = open(cx);
    add_from_background(&shell, cx);
    let (revision, _, _) = writes.try_recv().unwrap();
    assert_eq!(read(&shell, cx, "favouritesSaving", &[]), "true");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive_settings(
                Update::Failed {
                    through: revision,
                    keep: false,
                },
                window,
                cx,
            );
        })
    });
    assert_eq!(read(&shell, cx, "favouritesSaving", &[]), "false");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rail_favourite_controls_report_centres_while_drawn(cx: &mut TestAppContext) {
    let (shell, cx, _) = open(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.apply_settings(
                flea::uistate::from_file(
                    r#"{"view":"list","places":{"favourites":[{"path":"/files","label":"files"},{"path":"/other","label":"other"}]}}"#,
                ),
                window,
                cx,
            );
            shell.open_settings(window, cx);
            let view = shell.settings.panel.clone().expect("settings panel");
            view.update(cx, |view, cx| {
                view.panel.show_section("places");
                cx.notify();
            });
        })
    });
    settle(cx);
    let centre = |cx: &mut VisualTestContext, id: &str, part: &str| {
        read(&shell, cx, "settingsFavouriteControlCentre", &[id, part])
    };
    let drag = centre(cx, "favourite:0", "drag");
    let remove = centre(cx, "favourite:0", "remove");
    assert!(
        !drag.is_empty() && !remove.is_empty(),
        "favourite 0 draws both controls"
    );
    assert_ne!(drag, remove);
    assert_eq!(centre(cx, "favourite:9", "remove"), "");
    assert_eq!(centre(cx, "favourite:0", "rename"), "");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
