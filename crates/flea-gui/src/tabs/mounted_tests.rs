use super::transfer;
use crate::{
    backend::Backend,
    navigation::{folder_state::Order, sort::Snapshot},
    state::tests::shell_with,
};
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn u25_sorted_transfer_uses_source_snapshot_in_receivers_first_list(cx: &mut TestAppContext) {
    let (source_backend, _source_requests) = Backend::test_channel();
    let (backend, mut requests) = Backend::test_lines();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let source = cx.new(|cx| shell_with(source_backend, cx));
        let payload = source.update(cx, |shell, _| {
            shell.panes[0].as_mut().unwrap().nav.order = Order { key: "size", desc: true };
            shell.with_tabs(|_, panes| {
                let mut result = String::new();
                panes.visit(0, &mut |tabs, pane| result = tabs.tab_payload(pane, 0, "111", "sorted"));
                result
            })
        });
        let receiver = cx.new(|cx| shell_with(backend, cx));
        receiver.update(cx, |shell, cx| {
            assert_eq!(shell.panes[0].as_ref().unwrap().nav.order, Order::default());
            assert!(shell.with_tabs(|bar, panes| bar.strip_drop(panes, &payload, 0., 1)));
            while requests().is_some() {}
            let document = flea::jsondoc::parse(r#"{"t":"peeked","path":"/files","first":2,"n":0,"rows":[],"hidden":false,"hiddenLast":false}"#).unwrap();
            shell.receive(0, crate::backend::Event::Peeked(document), window, cx);
            let line = std::iter::from_fn(&mut requests).find(|line| line.contains("\"c\":\"list\"")).unwrap();
            let listing = flea::jsondoc::parse(&line).unwrap();
            assert_eq!(listing.get("by").and_then(flea::jsondoc::Json::as_str), Some("size"));
            assert_eq!(listing.get("desc").and_then(flea::jsondoc::Json::as_bool), Some(true));
            assert_eq!(shell.panes[0].as_ref().unwrap().nav.counters.lists, 1);
            assert_eq!(shell.panes[0].as_ref().unwrap().nav.counters.sorts, 0);
            assert_eq!(shell.panes[0].as_ref().unwrap().nav.counters.persisted, 0);
        });
    });
}

#[test]
fn u25_sort_payload_rejects_wrong_path_and_unknown_order() {
    let snapshot = Snapshot {
        path: "/files".into(),
        order: Order {
            key: "size",
            desc: true,
        },
    };
    let text = transfer::payload_with_sort("111", "sorted", "list", "", &snapshot);
    assert!(transfer::parse(&text).is_some());
    assert!(transfer::parse(&text.replace("\"key\":\"size\"", "\"key\":\"mode\"")).is_none());
    assert!(
        transfer::parse(&text.replacen("\"path\":\"/files\"", "\"path\":\"/wrong\"", 1)).is_none()
    );
    assert!(
        transfer::parse(&transfer::payload("111", "legacy", "/files", "list", ""))
            .unwrap()
            .sort
            .is_none()
    );
}

#[gpui::test]
fn u25_middle_click_uses_the_actual_grid_and_columns_paint(cx: &mut TestAppContext) {
    use crate::window::startup::View;
    use gpui::{point, px, Modifiers, MouseButton};
    for view in [View::Grid, View::Columns] {
        let (backend, _requests) = Backend::test_channel();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let mut shell = shell_with(backend, cx);
            let host = shell.panes[0].as_mut().unwrap();
            std::rc::Rc::get_mut(&mut host.pane.held).unwrap().rows[0] = flea::protocol::RowFacts {
                name: "sub".into(),
                dir: true,
                ..Default::default()
            };
            shell.set_view(view, window, cx);
            shell
        });
        cx.run_until_parked();
        if view == View::Columns {
            cx.update(|window, cx| shell.update(cx, |shell, cx| settle_columns(shell, window, cx)));
            cx.run_until_parked();
        }
        let at = cx.update(|_, cx| {
            let shell = shell.read(cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.drawn_view, Some(view));
            match view {
                View::Grid => {
                    let state = host.grid.as_ref().unwrap().state.borrow();
                    let tile = state.drawn().iter().find(|tile| tile.listing == 0).unwrap();
                    point(
                        state.bounds.origin.x + px(tile.slot.x + tile.slot.width / 2.),
                        state.bounds.origin.y + px(tile.slot.y + tile.slot.height / 2.),
                    )
                }
                View::Columns => {
                    let columns = host.columns.as_ref().unwrap();
                    let paint = columns
                        .painters
                        .get(crate::columns::geometry::Slot::Active)
                        .unwrap()
                        .borrow();
                    let drawn = paint.drawn.as_ref().unwrap();
                    point(
                        drawn.bounds.origin.x + px(40.),
                        drawn.bounds.origin.y + px(crate::list::pane::ROW_HEIGHT / 2.),
                    )
                }
                _ => unreachable!(),
            }
        });
        cx.simulate_mouse_down(at, MouseButton::Middle, Modifiers::default());
        cx.simulate_mouse_up(at, MouseButton::Middle, Modifiers::default());
        cx.run_until_parked();
        cx.update(|_, cx| {
            let shell = shell.read(cx);
            assert_eq!(shell.strip.tabs[0].count(), 2, "{view:?}");
            assert_eq!(
                shell.panes[0].as_ref().unwrap().pane.place(),
                "/files/sub",
                "{view:?}"
            );
        });
    }
}

fn settle_columns(
    shell: &mut crate::state::Shell,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<crate::state::Shell>,
) {
    let host = shell.panes[0].as_ref().unwrap();
    let columns = host.columns.as_ref().unwrap();
    let folders = host.nav.folders.borrow();
    let first = host.pane.first_count();
    let hidden = folders.hidden;
    let hidden_last = folders.hidden_last;
    let out = columns
        .area
        .drawn
        .iter()
        .filter(|path| {
            let key = crate::columns::peeks::peek_key(path, hidden, hidden_last);
            columns.area.peeks.rows(&key).is_none()
        })
        .cloned()
        .collect::<Vec<_>>();
    drop(folders);
    for path in out {
        let document = flea::jsondoc::parse(&format!(
            "{{\"t\":\"peeked\",\"path\":\"{}\",\"first\":{first},\"n\":1,\"rows\":[{{\"n\":\"x\",\"d\":true}}],\"hidden\":{hidden},\"hiddenLast\":{hidden_last}}}", flea::json::escape(&path)
        )).unwrap();
        shell.receive(0, crate::backend::Event::Peeked(document), window, cx);
    }
}

#[gpui::test]
fn u25_closing_the_window_cleans_the_source_and_catcher(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.follow_tab_drags(window, cx);
            assert!(shell.tab_action("tabNew", window, cx));
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 0, 1));
            assert!(shell.tab_native_payload(cx).is_some());
            assert!(!shell.strip.bar.out_payload.is_empty());
            let catchers: Vec<_> = cx
                .windows()
                .into_iter()
                .filter(|handle| shell.strip.catchers.contains(*handle))
                .collect();
            assert!(!catchers.is_empty(), "a lift maps its catchers");
            assert!(
                shell.strip.native.is_some(),
                "the strip follows native drags before the close"
            );
            assert!(shell.strip.router.is_some());
            shell.close(window, cx);
            assert!(shell.strip.bar.out_payload.is_empty());
            assert!(!shell.strip.bar.out_active);
            assert!(shell.strip.bar.lifts.is_empty());
            assert!(shell.strip.native.is_none());
            assert!(
                shell.strip.router.is_none(),
                "a closed window leaves the drag router"
            );
            assert!(shell.strip.timer.is_none());
            assert!(!shell
                .strip
                .frames
                .borrow()
                .json()
                .contains("\"available\":true"));
            assert!(catchers.iter().all(|handle| !cx.windows().contains(handle)));
        })
    });
}

#[test]
fn u25_tear_off_carries_the_same_sort_snapshot_and_fresh_launch_retires_it() {
    use super::test_support::World;
    use transfer::{TAB_SORT_ENV, TEAR_OFF_ENV};
    let mut world = World::new(2);
    world.pane.order = Order {
        key: "size",
        desc: true,
    };
    let mut bar = world.bar();
    bar.tab_lift_began(&mut world, 1, 1);
    let tear = bar.tear_off_at(2).unwrap();
    let command = transfer::tear_off_command("/stub/flea", &tear);
    let values = command
        .get_envs()
        .filter_map(|(name, value)| Some((name.to_str()?, value?.to_str()?)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let launch = transfer::Launch::read(
        |name| values.get(name).map(|value| (*value).into()),
        &tear.path,
    );
    assert_eq!(launch.sort.as_ref().unwrap().order, world.pane.order);
    let mut fresh = std::process::Command::new("/stub/flea");
    for name in TEAR_OFF_ENV.into_iter().chain([TAB_SORT_ENV]) {
        fresh.env(name, "stale");
    }
    transfer::fresh_launch(&mut fresh);
    assert!(fresh.get_envs().all(|(_, value)| value.is_none()));
}

#[gpui::test]
fn u25_a_live_search_tears_off_its_resting_folder_with_its_order(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        shell.update(cx, |shell, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.nav.order = Order {
                key: "size",
                desc: true,
            };
            host.pane.state.search = true;
            let payload = shell.with_tabs(|_, panes| {
                let mut result = String::new();
                panes.visit(0, &mut |tabs, pane| {
                    result = tabs.tab_payload(pane, 0, "111", "searching")
                });
                result
            });
            let info = transfer::parse(&payload).unwrap();
            assert_eq!(
                info.sort,
                Some(Snapshot {
                    path: info.path.clone(),
                    order: Order {
                        key: "size",
                        desc: true
                    }
                }),
                "a search tab tears off its resting folder in its order"
            );
        });
    });
}

#[test]
fn u25_tear_off_hand_off_selects_only_on_the_launch_pane_once() {
    let settings = crate::window::startup::Settings::from_json(flea::jsondoc::parse("{}").unwrap());
    let mut host = super::host::Host::new(&settings, "/files");
    host.launch.cursor = "note.txt".into();
    assert_eq!(
        host.launch_select(0, 1, "/other"),
        None,
        "pane 0 is not the launch pane"
    );
    assert_eq!(
        host.launch_select(1, 1, "/files"),
        Some("/files/note.txt".into())
    );
    assert_eq!(
        host.launch_select(1, 1, "/files"),
        None,
        "a later start never replays the hand-off"
    );
}

#[gpui::test]
fn u25_a_tab_switch_in_its_own_order_keeps_the_switch_options(cx: &mut TestAppContext) {
    use super::state::Open;
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        for clear_at_once in [true, false] {
            let (backend, _requests) = Backend::test_channel();
            let shell = cx.new(|cx| shell_with(backend, cx));
            shell.update(cx, |shell, _| {
                let order = Order {
                    key: "size",
                    desc: true,
                };
                shell.with_tabs(|_, panes| {
                    panes.visit(0, &mut |_, pane| {
                        pane.open(
                            "/files/child",
                            Open {
                                keep_hidden: true,
                                clear_at_once,
                                supersede: true,
                                order: Some(order),
                            },
                        )
                    });
                });
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(host.nav.order, order, "the tab's own order lists");
                assert_eq!(
                    host.pane.swap.holding, !clear_at_once,
                    "a tab in another view clears at once"
                );
            });
        }
    });
}
