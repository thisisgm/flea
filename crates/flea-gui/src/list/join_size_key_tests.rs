//! A settle cannot spend stale geometry or ownership.
use super::{classify, folders, frame, next, quiet, receive, request, settle};
use crate::list::{bench, pane::Held};
use flea::jsondoc::{self, Json};
use gpui::AppContext;
use std::rc::Rc;

#[test]
fn u04_a_settle_rechecks_live_ownership_even_before_the_next_frame() {
    for change in [
        "scroll",
        "view",
        "search",
        "held",
        "columns",
        "stored",
        "strip",
        "density",
        "text_geometry",
        "font",
    ] {
        let mut cx = bench::real_cx();
        let handle = folders(&mut cx, "");
        classify(&mut cx, handle);
        handle
            .update(&mut cx, |shell, _, _| {
                let host = shell.panes[0].as_mut().unwrap();
                match change {
                    "scroll" => host.pane.scroll_y = 31.,
                    "density" => host.list.borrow_mut().density = crate::list::Density::Tight,
                    "text_geometry" => {
                        let mut list = host.list.borrow_mut();
                        let mut face = list.typeface.clone();
                        face.base = gpui::px(12.);
                        face.body_small = gpui::px(11.);
                        list.set_typeface(face);
                    }
                    "font" => {
                        let mut list = host.list.borrow_mut();
                        let mut face = list.typeface.clone();
                        face.font = gpui::font("monospace");
                        list.set_typeface(face);
                    }
                    "view" => shell.view = crate::window::startup::View::Grid,
                    "search" => host.pane.state.search = true,
                    "held" => {
                        host.pane.held = Rc::new(Held {
                            listing: 2,
                            total: 2000,
                            start: 0,
                            rows: bench::fixture("2k"),
                        });
                    }
                    "stored" => {
                        shell.launch.settings.data =
                            jsondoc::parse(r#"{"columnWidths":{"size":480}}"#).unwrap()
                    }
                    "strip" => shell.slots.strip_height = 100.,
                    "columns" => {
                        shell.launch.settings.data =
                            jsondoc::parse(r#"{"columns":["mode"]}"#).unwrap()
                    }
                    _ => unreachable!(),
                }
            })
            .unwrap();
        settle(&mut cx);
        quiet(&mut cx, handle);
    }
}

struct Strip;
impl gpui::Render for Strip {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div()
    }
}

#[test]
fn u04_a_strip_height_change_settles_newly_exposed_rows_at_the_same_pane_size() {
    let mut cx = bench::real_cx();
    let handle = folders(&mut cx, "");
    handle
        .update(&mut cx, |shell, _, cx| {
            shell.slots.strip_height = 550.;
            shell.slots.strips[0] = Some(cx.new(|_| Strip).into());
            let host = shell.panes[0].as_mut().unwrap();
            let mut rows = bench::fixture("2k");
            rows.truncate(20);
            for row in &mut rows {
                row.dir = true;
                row.mode = 0o040755;
            }
            host.pane.held = Rc::new(Held {
                listing: 2,
                total: 20,
                start: 0,
                rows,
            });
            host.pane.scroll_y = 551.;
        })
        .unwrap();
    classify(&mut cx, handle);
    settle(&mut cx);
    let command = request(&mut cx, handle);
    let rows = command.get("rows").and_then(Json::as_array).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows.first().and_then(Json::as_f64), Some(17.));
    assert_eq!(rows.last().and_then(Json::as_f64), Some(19.));
    for _ in rows {
        let event = next(&mut cx, handle);
        receive(&mut cx, handle, event);
    }
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |shell, _, _| shell.slots.strip_height = 0.)
        .unwrap();
    frame(&mut cx, handle);
    settle(&mut cx);
    let command = request(&mut cx, handle);
    let rows = command.get("rows").and_then(Json::as_array).unwrap();
    assert_eq!(rows.len(), 17);
    assert_eq!(rows.first().and_then(Json::as_f64), Some(0.));
    assert_eq!(rows.last().and_then(Json::as_f64), Some(16.));
}

#[test]
fn u04_visible_window_requests_precede_background_filesystem_details() {
    let mut cx = bench::real_cx();
    let handle = folders(&mut cx, "");
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let mut rows = bench::fixture("2k");
            rows.truncate(320);
            host.pane.held = Rc::new(Held {
                listing: 2,
                total: 2000,
                start: 0,
                rows,
            });
            host.pane.scroll_y = 600. * 31.;
        })
        .unwrap();
    frame(&mut cx, handle);
    let window = request(&mut cx, handle);
    assert_eq!(window.get("c").and_then(Json::as_str), Some("window"));
    assert_eq!(window.get("start").and_then(Json::as_f64), Some(450.));
    assert_eq!(
        request(&mut cx, handle).get("c").and_then(Json::as_str),
        Some("fsinfo")
    );
}

#[path = "join_presentation_geometry_tests.rs"]
mod presentation_geometry;
