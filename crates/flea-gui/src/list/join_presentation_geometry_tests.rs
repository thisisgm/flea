//! Row geometry changes preserve the viewport and replan real backend size requests.
use super::{classify, folders, frame, next, quiet, receive, request, settle};
use crate::{
    list::{bench, pane::Held},
    presentation::Fonts,
    window::startup::{Settings, View},
};
use flea::jsondoc::{self, Json};
use gpui::{TestAppContext, WindowHandle};
use std::rc::Rc;

fn fonts(base: u32) -> Fonts {
    Fonts::from_sources(
        &format!("[font]\nbase-size={base}\nbody=14"),
        "",
        "JetBrains Mono".into(),
    )
}

fn change_density(
    cx: &mut TestAppContext,
    handle: WindowHandle<crate::state::Shell>,
    density: &str,
) {
    handle
        .update(cx, |shell, window, cx| {
            let patch = jsondoc::parse(&format!(r#"{{"density":"{density}"}}"#)).unwrap();
            let data = flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
            let host = shell.panes[0].as_ref().unwrap();
            let held = Rc::clone(&host.pane.held);
            let requests = host.pane.list_requests;
            shell.apply_settings(data, window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                host.pane.list_requests, requests,
                "a density-only change started a listing request"
            );
            assert!(
                !host.pane.busy(),
                "a density-only change left a listing pending"
            );
            assert!(Rc::ptr_eq(&held, &host.pane.held));
        })
        .unwrap();
}

fn requested_rows(command: &Json) -> Vec<usize> {
    assert_eq!(command.get("c").and_then(Json::as_str), Some("dirsize"));
    command
        .get("rows")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .map(|row| row.as_f64().unwrap() as usize)
        .collect()
}

#[test]
fn g70_geometry_change_requests_newly_visible_sizes_without_reasking_answered_rows() {
    for change in ["density", "text"] {
        let mut cx = bench::real_cx();
        cx.update(|cx| cx.set_global(fonts(14)));
        let handle = folders(&mut cx, "");
        handle
            .update(&mut cx, |shell, _, _| {
                // Match startup's full settings shape before applying a density-only patch.
                shell.launch.settings = Settings::from_json(flea::uistate::from_file("{}"));
                let host = shell.panes[0].as_mut().unwrap();
                let mut rows = bench::fixture("2k");
                for row in rows.iter_mut().take(64) {
                    row.dir = true;
                    row.mode = 0o040755;
                }
                host.pane.held = Rc::new(Held {
                    listing: 2,
                    total: rows.len(),
                    start: 0,
                    rows,
                });
            })
            .unwrap();
        change_density(&mut cx, handle, "normal");
        classify(&mut cx, handle);
        settle(&mut cx);
        let answered = requested_rows(&request(&mut cx, handle));
        assert!(!answered.is_empty());
        for _ in &answered {
            let event = next(&mut cx, handle);
            receive(&mut cx, handle, event);
        }
        frame(&mut cx, handle);
        settle(&mut cx);
        quiet(&mut cx, handle);

        let held = handle
            .update(&mut cx, |shell, _, _| {
                Rc::clone(&shell.panes[0].as_ref().unwrap().pane.held)
            })
            .unwrap();
        match change {
            "density" => change_density(&mut cx, handle, "tight"),
            "text" => {
                handle
                    .update(&mut cx, |shell, _, cx| {
                        cx.set_global(fonts(12));
                        shell.sync_font_settings(cx);
                    })
                    .unwrap();
            }
            _ => unreachable!(),
        }
        frame(&mut cx, handle);
        let expected = handle
            .update(&mut cx, |shell, _, _| {
                let host = shell.panes[0].as_ref().unwrap();
                let list = host.list.borrow();
                assert!(Rc::ptr_eq(&held, &host.pane.held));
                assert_eq!(host.pane.scroll_y, 0.);
                assert_eq!(f32::from(list.typeface.body), 14.);
                list.visible()
                    .iter()
                    .filter(|row| !answered.contains(&row.index))
                    .filter(|row| held.row(row.index).is_some_and(|facts| facts.dir))
                    .map(|row| row.index)
                    .collect::<Vec<_>>()
            })
            .unwrap();
        assert!(
            !expected.is_empty(),
            "{change} did not expose another folder"
        );
        settle(&mut cx);
        let requested = requested_rows(&request(&mut cx, handle));
        assert_eq!(requested, expected, "{change}");
        for _ in &requested {
            let event = next(&mut cx, handle);
            receive(&mut cx, handle, event);
        }
        frame(&mut cx, handle);
        settle(&mut cx);
        quiet(&mut cx, handle);
    }
}

#[test]
fn g70_live_font_change_keeps_the_bottom_rows_before_and_after_paint() {
    let mut cx = bench::real_cx();
    cx.update(|cx| cx.set_global(fonts(14)));
    let handle = super::super::super::open(&mut cx);
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let rows: Vec<_> = bench::fixture("2k").into_iter().take(100).collect();
            host.pane.held = Rc::new(Held {
                listing: 2,
                total: rows.len(),
                start: 0,
                rows,
            });
        })
        .unwrap();
    frame(&mut cx, handle);
    let anchor = handle
        .update(&mut cx, |shell, _, _| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            assert!(pane.scroll_extent.is_some());
            pane.scroll_to(f32::MAX);
            pane.scroll_y / pane.row_height()
        })
        .unwrap();
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |shell, _, cx| {
            let family = shell.active_typeface().font.family.to_string();
            cx.set_global(Fonts::from_sources("[font]\nbase-size=20", "", family));
            shell.sync_font_settings(cx);
            let pane = &shell.panes[0].as_ref().unwrap().pane;
            assert!((pane.scroll_y / pane.row_height() - anchor).abs() < 0.001);
        })
        .unwrap();
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let list = host.list.borrow();
            assert!((list.drawn.scroll_y / list.drawn.metrics.row - anchor).abs() < 0.001);
            assert_eq!(
                host.pane.scroll_extent,
                Some(100. * list.drawn.metrics.row + list.drawn.metrics.header)
            );
        })
        .unwrap();
}

#[test]
fn g70_font_change_leaves_grid_owned_scroll_geometry_unchanged() {
    let mut cx = bench::real_cx();
    cx.update(|cx| cx.set_global(fonts(14)));
    let handle = super::super::super::open(&mut cx);
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |shell, _, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            let grid = host.grid.as_mut().unwrap();
            grid.view.scroll = 500.;
            let layout = grid
                .view
                .layout(crate::grid::Shown::All(host.pane.held.total));
            let family = host.list.borrow().typeface.font.family.to_string();
            cx.set_global(Fonts::from_sources("[font]\nbase-size=20", "", family));
            shell.sync_font_settings(cx);
            let host = shell.panes[0].as_ref().unwrap();
            let grid = host.grid.as_ref().unwrap();
            let after = grid
                .view
                .layout(crate::grid::Shown::All(host.pane.held.total));
            assert_eq!(grid.view.scroll, 500.);
            assert_eq!(after.columns, layout.columns);
            assert_eq!(after, layout);
        })
        .unwrap();
}

#[path = "header_shrink_tests.rs"]
mod header_shrink;
