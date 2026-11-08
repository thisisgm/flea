//! Live Columns regressions: the owned IPC call must reach the window's mounted tracks.
use crate::{backend::Backend, ipc::server::Event, state::tests::shell_with};
use flea::ipc::wire::{Reply, Request};
use gpui::{AppContext, TestAppContext};

#[path = "columns_capture_tests.rs"]
mod capture;
#[path = "columns_density_tests.rs"]
mod density;
#[path = "columns_density_search_tests.rs"]
mod density_search;
#[path = "columns_empty_middle_tests.rs"]
mod empty_middle;
#[path = "columns_finder_tests.rs"]
mod finder;
#[path = "columns_fixlist_tests.rs"]
mod fixlist;
#[path = "columns_gesture_tests.rs"]
mod gestures;
#[path = "columns_test_host.rs"]
pub(crate) mod host;
#[path = "columns_hover_tests.rs"]
mod hover;
#[path = "columns_neighbour_click_tests.rs"]
mod neighbour_click;
#[path = "columns_neighbour_tab_tests.rs"]
mod neighbour_tab;
#[path = "columns_pointer_tests.rs"]
mod pointer;
#[path = "columns_review_tests.rs"]
mod review;
#[path = "columns_scroll_tests.rs"]
pub(crate) mod scroll;
#[path = "columns_state_tests.rs"]
mod state;
#[path = "columns_tap_tests.rs"]
mod tap;
#[path = "columns_wheel_tests.rs"]
mod wheel;

#[test]
fn u09_live_columns_width_reader_observes_the_list_producers_stored_values() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        shell.set_view(super::startup::View::List, window, cx);
        for (written, expected) in [(190, 190.), (1, 48.)] {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow_mut()
                .write_width(crate::list::columns::Column::Size, written);
            let reply = host::call(shell, "columnWidths", &[], window, cx);
            let widths = flea::jsondoc::parse(&reply.stdout).unwrap();
            assert_eq!(
                widths.get("size").and_then(flea::jsondoc::Json::as_f64),
                Some(expected)
            );
            assert!(
                widths.get("kind").is_none(),
                "the reader invented an unstored width"
            );
        }
    });
}
#[path = "columns_watch_tests.rs"]
mod watch;

#[test]
fn u09_live_columns_paint_real_tracks_and_read_only_visible_geometry() {
    use crate::columns::geometry::Slot;
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(
            shell,
            "/srv/a/b",
            &[("c", true), ("sibling", true)],
            false,
            window,
            cx,
        );
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("seed.txt", false)],
            false,
            window,
            cx,
        );
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        assert_eq!(
            host::call(shell, "columnCount", &[], window, cx)
                .stdout
                .trim(),
            "3"
        );
        assert_eq!(
            host::call(shell, "columnNames", &["child"], window, cx)
                .stdout
                .trim(),
            "seed.txt"
        );
        assert_eq!(
            host::call(shell, "columnChildRowName", &["0"], window, cx)
                .stdout
                .trim(),
            "seed.txt"
        );
        assert!(host::call(shell, "columnChildRowName", &["1"], window, cx)
            .stdout
            .trim()
            .is_empty());
        let active = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let scroll = active
            .painters
            .get(Slot::Active)
            .and_then(|paint| paint.borrow().drawn.as_ref().map(|drawn| drawn.scroll))
            .expect("the active column drew");
        assert_eq!(
            host::call(shell, "viewContentY", &[], window, cx)
                .stdout
                .trim(),
            scroll.round().to_string()
        );
        let centre = host::call(shell, "columnParentRowCentre", &["0"], window, cx).stdout;
        let values: Vec<f32> = centre
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        assert_eq!(columns.painters.built(), 3);
        let painter = columns.painters.get(Slot::Parent).unwrap().borrow();
        let drawn = painter.drawn.as_ref().unwrap();
        assert_eq!(
            f32::from(drawn.bounds.origin.y),
            27.,
            "Columns has no list header"
        );
        assert_eq!(
            values,
            [
                f32::from(drawn.bounds.origin.x + drawn.bounds.size.width / 2.).round(),
                43.
            ]
        );
        assert!(painter.prepared(0).unwrap().dir);
        assert!(f32::from(painter.prepared(0).unwrap().plain.line().width) > 0.);
        drop(painter);
        for (function, index) in [
            ("columnParentRowCentre", "100"),
            ("columnChildRowCentre", "1"),
        ] {
            assert!(host::call(shell, function, &[index], window, cx)
                .stdout
                .trim()
                .is_empty());
        }
        assert!(
            host::call(shell, "columnGrandparentRowCentre", &["0"], window, cx)
                .stdout
                .trim()
                .is_empty()
        );
    });
}

#[test]
fn u09_live_columns_root_slots_are_blank_but_real_ancestors_paint() {
    for (width, count) in [(1400., 3), (2100., 4), (2700., 5)] {
        let mut cx = host::context();
        let (window, _requests) = host::open_at(&mut cx, "/", width, count);
        host::change(&mut cx, window, host::settle);
        host::frame(&mut cx, window);
        host::change(&mut cx, window, |shell, window, cx| {
            assert_eq!(
                host::call(shell, "columnCount", &[], window, cx)
                    .stdout
                    .trim(),
                count.to_string()
            );
            for function in [
                "columnParentRowCentre",
                "columnGrandparentRowCentre",
                "columnGreatGrandparentRowCentre",
            ] {
                assert!(
                    host::call(shell, function, &["0"], window, cx)
                        .stdout
                        .trim()
                        .is_empty(),
                    "{function} drew a duplicate root"
                );
            }
        });
        let (window, _requests) = host::open_at(&mut cx, "/srv/a/b/c", width, count);
        host::change(&mut cx, window, |shell, window, cx| {
            for (path, name) in [("/srv/a/b", "c"), ("/srv/a", "b"), ("/srv", "a")] {
                host::peek(shell, path, &[(name, true)], false, window, cx);
            }
            host::settle(shell, window, cx);
        });
        host::frame(&mut cx, window);
        host::change(&mut cx, window, |shell, window, cx| {
            for function in [
                "columnParentRowCentre",
                "columnGrandparentRowCentre",
                "columnGreatGrandparentRowCentre",
            ]
            .iter()
            .take(count - 2)
            {
                assert!(
                    !host::call(shell, function, &["0"], window, cx)
                        .stdout
                        .trim()
                        .is_empty(),
                    "{function} has no positive control"
                );
            }
        });
    }
}

#[gpui::test]
fn u09_live_columns_readers_forward_the_retained_preview_state(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.set_view(super::startup::View::Columns, window, cx);
            for (function, arguments) in [
                ("columnCount", vec![]),
                ("columnWidths", vec![]),
                ("columnNames", vec!["parent".into()]),
                ("columnPeekNames", vec!["/".into()]),
                ("columnParentRowCentre", vec!["0".into()]),
                ("columnGrandparentRowCentre", vec!["0".into()]),
                ("columnGreatGrandparentRowCentre", vec!["0".into()]),
                ("columnChildRowCentre", vec!["0".into()]),
                ("columnChildEmpty", vec![]),
            ] {
                let (answer, receive) = std::sync::mpsc::sync_channel(1);
                shell.ipc(
                    Event::Call {
                        request: Request::Call {
                            target: "flea".into(),
                            function: function.into(),
                            arguments,
                        },
                        answer,
                    },
                    window,
                    cx,
                );
                assert_eq!(
                    receive.try_recv().unwrap().reply.code,
                    0,
                    "{function} is detached from the live window"
                );
            }
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: "columnPdfPages".into(),
                        arguments: vec![],
                    },
                    answer,
                },
                window,
                cx,
            );
            assert_eq!(receive.try_recv().unwrap().reply, Reply::value("0"));
        });
    });
}

#[path = "columns_parity_build_tests.rs"]
mod parity_build;
