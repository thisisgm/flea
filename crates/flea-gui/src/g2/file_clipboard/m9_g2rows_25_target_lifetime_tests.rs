//! Painted targets cannot outlive their geometry, modal gate or asynchronous peek snapshot.
use super::m9_g2rows_25_rig::{answer, edit, open, riding};
use super::m9_g2rows_25_target_rig::{down, peek, target, view, Fixture};
use crate::{columns::geometry::Slot, window::startup::View};
use std::rc::Rc;

#[test]
fn m9_g2rows_25_mounted_stale_geometry_and_modal_clicks_keep_the_carry() {
    for mode in [View::List, View::Grid, View::Columns] {
        for gate in ["listing", "modal", "busy", "stopped"] {
            let fixture = Fixture::new();
            let mut cx = crate::list::bench::real_cx();
            let (from, mut source_lines) = open(&mut cx, &fixture.path("src"), &["a.txt"]);
            let (to, mut lines) = open(&mut cx, &fixture.path("dest"), &["folder", "a.txt"]);
            view(&mut cx, to, mode);
            source_lines();
            lines();
            edit(&mut cx, from, |shell, window, cx| {
                shell.carry_lift(window, cx)
            });
            source_lines();
            answer(&mut cx, from, &[&fixture.path("src/a.txt")]);
            let at = target(&mut cx, to, Some(0), Slot::Active);
            edit(&mut cx, to, |shell, window, cx| {
                let host = shell.panes[0].as_mut().unwrap();
                match gate {
                    "listing" => {
                        host.pane.held = Rc::new(crate::list::pane::Held {
                            listing: host.pane.held.listing + 1,
                            total: host.pane.held.total,
                            start: host.pane.held.start,
                            rows: host.pane.held.rows.clone(),
                        })
                    }
                    "modal" => shell.input.blocked = true,
                    "busy" => {
                        assert!(host.pane.begin(
                            fixture.path("src"),
                            Default::default(),
                            host.pane.now()
                        ));
                    }
                    "stopped" => host.stopped = true,
                    _ => unreachable!(),
                }
                down(shell, at, window, cx);
            });
            cx.run_until_parked();
            assert!(lines().is_empty(), "INVALID_TARGET_ASKED {mode:?} {gate}");
            assert!(riding(&mut cx).is_some());
        }
    }
}

#[test]
fn m9_g2rows_25_mounted_columns_peek_change_during_facts_cancels_the_click() {
    for change in ["peek", "modal", "view"] {
        let fixture = Fixture::new();
        let mut cx = crate::list::bench::real_cx();
        let (from, mut source_lines) = open(&mut cx, &fixture.path("src"), &["a.txt"]);
        let (to, mut lines) = open(&mut cx, &fixture.path("dest"), &["folder", "a.txt"]);
        view(&mut cx, to, View::Columns);
        source_lines();
        lines();
        edit(&mut cx, from, |shell, window, cx| {
            shell.carry_lift(window, cx)
        });
        source_lines();
        answer(&mut cx, from, &[&fixture.path("src/a.txt")]);
        let at = target(&mut cx, to, Some(0), Slot::Child);
        edit(&mut cx, to, |shell, window, cx| {
            down(shell, at, window, cx);
            match change {
                "peek" => {
                    // Only an asked reply lands, so the watch asks the drawn child again first, as a real change does.
                    let path = fixture.path("dest/folder");
                    shell.receive(
                        0,
                        crate::backend::Event::Changed { path: path.clone() },
                        window,
                        cx,
                    );
                    peek(shell, &path, &[("replacement", true)], window, cx);
                }
                "modal" => shell.input.blocked = true,
                "view" => {
                    shell.set_view(View::List, window, cx);
                }
                _ => unreachable!(),
            }
        });
        cx.run_until_parked();
        let sent = lines();
        assert!(
            !sent.iter().any(|line| line.contains(r#""c":"collisions""#)),
            "STALE_COLUMN_TARGET_ASKED {change}: {sent:?}"
        );
        assert!(riding(&mut cx).is_some());
    }
}
