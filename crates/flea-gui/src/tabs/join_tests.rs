use crate::{backend::Backend, ipc::server::Event, state::tests::shell_with};
use flea::ipc::wire::{Reply, Request};
use gpui::{AppContext, KeyDownEvent, Keystroke, TestAppContext};

pub(super) fn read(
    shell: &mut crate::state::Shell,
    function: &str,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<crate::state::Shell>,
) -> Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    shell.ipc(
        Event::Call {
            request: Request::Call {
                target: "flea".into(),
                function: function.into(),
                arguments: vec![],
            },
            answer,
        },
        window,
        cx,
    );
    receive.try_recv().unwrap().reply
}

#[gpui::test]
fn u25_native_tab_actions_reach_the_mounted_strip(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            assert_eq!(read(shell, "tabCount", window, cx), Reply::value("1"));
            key(shell, "t", window, cx);
            assert_eq!(read(shell, "tabCount", window, cx), Reply::value("2"));
            assert_eq!(read(shell, "tabIndex", window, cx), Reply::value("1"));
            assert!(shell.has_tabs());
        });
    });
}

pub(super) fn key(
    shell: &mut crate::state::Shell,
    chord: &str,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<crate::state::Shell>,
) {
    let mut keystroke = Keystroke::parse(chord).unwrap();
    if !keystroke.modifiers.control {
        keystroke.key_char = Some(keystroke.key.clone());
    }
    shell.key(
        &KeyDownEvent {
            keystroke,
            is_held: false,
            prefer_character_input: false,
        },
        window,
        cx,
    );
}

pub(super) fn land(
    shell: &mut crate::state::Shell,
    path: &str,
    names: &[(&str, bool)],
    listing: usize,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<crate::state::Shell>,
) {
    use crate::backend::Event as BackendEvent;
    use flea::protocol::{Reply, RowFacts};
    shell.receive(
        0,
        BackendEvent::Reply(Reply::Listed {
            path: path.into(),
            total: names.len(),
        }),
        window,
        cx,
    );
    shell.receive(
        0,
        BackendEvent::Reply(Reply::Rows {
            start: 0,
            rows: names
                .iter()
                .map(|(name, dir)| RowFacts {
                    name: (*name).into(),
                    dir: *dir,
                    ..RowFacts::default()
                })
                .collect(),
            listing,
        }),
        window,
        cx,
    );
    let host = shell.panes[0].as_mut().unwrap();
    if host.pane.busy() {
        host.pane.tick(host.pane.now() + 1000);
        host.changed(&shell.locale);
        shell.tab_landed(0, window, cx);
    }
}

#[gpui::test]
fn u25_cursor_folder_chord_refuses_files_and_restores_the_departed_cursor(cx: &mut TestAppContext) {
    use std::rc::Rc;
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            let held = Rc::get_mut(&mut host.pane.held).unwrap();
            held.total = 3;
            held.rows = vec![
                flea::protocol::RowFacts {
                    name: "alpha".into(),
                    dir: true,
                    ..Default::default()
                },
                flea::protocol::RowFacts {
                    name: "note.txt".into(),
                    ..Default::default()
                },
                flea::protocol::RowFacts {
                    name: "beta".into(),
                    dir: true,
                    ..Default::default()
                },
            ];
            host.pane.cursor = 1;
            key(shell, "ctrl-enter", window, cx);
            assert_eq!(shell.strip.tabs[0].count(), 1);
            assert_eq!(
                shell.strip.notice[0].text,
                shell.locale.catalog.tr("tabs.only_folder")
            );
            shell.panes[0].as_mut().unwrap().pane.cursor = 2;
            key(shell, "ctrl-enter", window, cx);
            assert_eq!(shell.strip.tabs[0].count(), 2);
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.place(), "/files/beta");
            land(shell, "/files/beta", &[], 3, window, cx);
            key(shell, "ctrl-pageup", window, cx);
            land(
                shell,
                "/files",
                &[("alpha", true), ("note.txt", false), ("beta", true)],
                4,
                window,
                cx,
            );
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 2);
            assert_eq!(shell.strip.tabs[0].current(), 0);
        });
    });
}

#[gpui::test]
fn u25_reorder_lists_nothing_and_count_refusals_preserve_state(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            key(shell, "w", window, cx);
            assert_eq!(shell.strip.tabs[0].count(), 1);
            assert_eq!(
                shell.strip.notice[0].text,
                shell.locale.catalog.tr("tabs.last_tab")
            );
            key(shell, "t", window, cx);
            assert!(shell.strip.notice[0].text.is_empty());
            key(shell, "t", window, cx);
            let lists = shell.panes[0].as_ref().unwrap().pane.list_requests;
            let identity = shell.strip.tabs[0].tabs.as_ref().unwrap().items[2].identity;
            key(shell, "{", window, cx);
            assert_eq!(shell.strip.tabs[0].current(), 1);
            assert_eq!(
                shell.strip.tabs[0].tabs.as_ref().unwrap().items[1].identity,
                identity
            );
            key(shell, "}", window, cx);
            assert_eq!(shell.strip.tabs[0].current(), 2);
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.list_requests, lists);
            for _ in 3..9 {
                key(shell, "t", window, cx);
            }
            let before = shell.strip.tabs[0].tabs.clone();
            key(shell, "t", window, cx);
            assert_eq!(shell.strip.tabs[0].tabs, before);
            assert_eq!(
                shell.strip.notice[0].text,
                shell.locale.catalog.tr("tabs.most")
            );
        });
    });
}

#[gpui::test]
fn u25_strip_pointer_reads_painted_geometry_and_middle_closes(cx: &mut TestAppContext) {
    use gpui::{point, px, Modifiers, MouseButton};
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| shell.update(cx, |shell, cx| key(shell, "t", window, cx)));
    cx.run_until_parked();
    let (centre, writes) = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let bounds = shell
                .strip
                .bounds
                .get()
                .expect("the actual painter recorded its strip");
            assert_eq!(f32::from(bounds.origin.y), 27.);
            assert_eq!(
                read(shell, "tabBarVisible", window, cx),
                Reply::value("true")
            );
            let label = shell.locale.catalog.tr("tabs.home");
            use crate::ipc::Adapter;
            let centre = shell
                .tab_adapter(&label)
                .call("tabCentre", &["0".into()])
                .unwrap();
            let parts: Vec<f32> = centre
                .stdout
                .split_whitespace()
                .map(|part| part.parse().unwrap())
                .collect();
            (
                point(px(parts[0]), px(parts[1])),
                shell.strip.remember.writes,
            )
        })
    });
    cx.simulate_mouse_down(centre, MouseButton::Middle, Modifiers::default());
    cx.simulate_mouse_up(centre, MouseButton::Middle, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.strip.tabs[0].count(), 1);
            assert_eq!(
                read(shell, "tabBarVisible", window, cx),
                Reply::value("false")
            );
            assert!(shell.strip.bounds.get().is_none());
            assert_eq!(shell.strip.remember.writes, writes + 1);
        })
    });
}

#[test]
fn u25_last_folder_reopen_chooses_saved_index_and_named_launch_ignores_it() {
    use crate::window::startup::{Arguments, Launch, Settings};
    let settings = || {
        Settings::from_json(flea::jsondoc::parse(r#"{"startIn":"last","lastPath":"/old","lastTabs":{"paths":["/one","/two"],"index":1}}"#).unwrap())
    };
    let reopened = Launch::resolve(settings(), Arguments::default(), "/home/test");
    assert_eq!(reopened.paths[0], "/two");
    assert_eq!(reopened.tabs.unwrap().index, 1);
    let named = Launch::resolve(
        settings(),
        Arguments {
            path: Some("/named".into()),
            select: None,
        },
        "/home/test",
    );
    assert!(named.tabs.is_none());
    assert_eq!(named.paths[0], "/named");
}

#[path = "join_transfer_tests.rs"]
mod transfers;

#[gpui::test]
fn u25_later_navigation_supersedes_the_saved_tab_cursor(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            std::rc::Rc::get_mut(&mut host.pane.held).unwrap().rows[200].name = "target.txt".into();
            host.pane.cursor = 200;
            shell.open_tab(0, "/other".into(), window, cx);
            land(shell, "/other", &[("one", false)], 3, window, cx);
            key(shell, "ctrl-pageup", window, cx);
            assert!(
                std::iter::from_fn(&mut requests)
                    .any(|line| line.contains("\"select\":\"/files/target.txt\"")),
                "the initial window request carries the saved name"
            );
            assert_eq!(
                shell.step(
                    0,
                    crate::window::navigate::Step::Open("/superseding".into()),
                    window,
                    cx
                ),
                Ok(true)
            );
            land(
                shell,
                "/superseding",
                &[("one", false), ("two", false)],
                4,
                window,
                cx,
            );
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 0);
            assert_eq!(
                shell.strip.tabs[0].tabs.as_ref().unwrap().pending,
                Default::default()
            );
        });
    });
}

#[gpui::test]
fn u25_native_drop_zone_tracks_the_drawn_strip_and_full_capacity_own_return(
    cx: &mut TestAppContext,
) {
    use gpui::{point, px};
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell
                .strip
                .drop_zone
                .get()
                .unwrap()
                .contains(&point(px(40.), px(120.))));
            for _ in 1..9 {
                key(shell, "t", window, cx);
            }
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.strip.drop_zone.get(), shell.strip.bounds.get());
            assert!(!shell
                .strip
                .drop_zone
                .get()
                .unwrap()
                .contains(&point(px(40.), px(120.))));
            assert!(!shell.strip.capacity.get());
            let now = shell.tab_now();
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 0, now));
            assert!(shell.tab_native_payload(cx).is_some());
            assert!(
                shell.strip.capacity.get(),
                "own native return requires no free slot"
            );
            let payload = shell.strip.bar.out_payload.clone();
            assert!(shell.with_tabs(|bar, panes| bar.strip_drop(panes, &payload, 0., now)));
            shell.strip.bar.out_finished(now);
            assert_eq!(shell.strip.tabs[0].count(), 9);
            // WindowBody.qml: Dual never takes a window-wide drop; its drawn strip keeps its own DropArea.
            shell.view = crate::window::startup::View::Dual;
            shell.sync_tabs(window, cx);
            assert_eq!(shell.strip.drop_zone.get(), shell.strip.bounds.get());
            assert!(!shell
                .strip
                .drop_zone
                .get()
                .unwrap()
                .contains(&point(px(40.), px(120.))));
        })
    });
}

// EY-469: Enter below a strip must admit the tab so a later strip drop can land.
#[gpui::test]
fn ey469_a_tab_entering_below_a_shown_strip_drops_on_it(cx: &mut TestAppContext) {
    use super::transfer;
    use crate::g2::dg2::router::{self, Native, Offered};
    use gpui::{point, px};
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.follow_tab_drags(window, cx);
            key(shell, "t", window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let tabs = shell.geometry(window).tabs;
            let right = f32::from(window.viewport_size().width) * 0.97;
            let strip = (right, tabs.y + tabs.height / 2.);
            let below = (right, tabs.y + tabs.height + 200.);
            let zone = shell.strip.drop_zone.get().unwrap();
            assert!(zone.contains(&point(px(strip.0), px(strip.1))));
            assert!(!zone.contains(&point(px(below.0), px(below.1))));
            let id = window.window_handle().window_id().as_u64();
            let shared = router::shared(cx);
            let types = [transfer::TAB_MIME.to_string()];
            let entered = shared.offer(&Offered {
                window: id,
                position: below,
                types: &types,
            });
            assert!(entered.move_, "the Enter below the strip takes the tab");
            let payload = transfer::payload("222", "lift-9", "/files/child", "list", "");
            let data = |mime: &str| {
                (mime == transfer::TAB_MIME).then(|| std::sync::Arc::from(payload.as_bytes()))
            };
            let at = |position, dropped| {
                Native::new(id, position, dropped, &types, &data, None, false, false)
            };
            assert!(shared.land(&at(below, false)), "the Enter is admitted");
            assert!(
                !shared.land(&at(below, true)),
                "a drop off the strip is refused"
            );
            assert!(shared.land(&at(strip, true)), "a drop on the strip lands");
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let pending = shell
            .read(cx)
            .strip
            .bar
            .pending
            .as_ref()
            .map(|p| (p.token.clone(), p.at));
        assert_eq!(pending, Some(("lift-9".to_string(), Some(2))));
    });
}

#[path = "atomic_tests.rs"]
mod atomic;
#[path = "catcher_tests.rs"]
mod catcher_tests;
#[path = "fix2_tests.rs"]
mod fix2_tests;
#[path = "notice_tests.rs"]
mod notice_tests;
