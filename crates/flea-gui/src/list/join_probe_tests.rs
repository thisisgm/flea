//! Header geometry reads the title run's text and clipping, including its cached sort direction.
use super::{frame, open};
use crate::{
    list::{
        bench,
        columns::{self, StoredWidths, Switched, Tokens},
        header::HeaderState,
        Metrics,
    },
    visual::Probe,
    window::startup::View,
};
use flea::jsondoc::Json;
use gpui::px;

fn role(probe: &Probe, key: &str) -> Option<Json> {
    probe
        .json(&Json::Null)
        .get("roles")?
        .as_array()?
        .iter()
        .find(|role| role.get("id").and_then(Json::as_str) == Some(key))
        .cloned()
}

#[test]
fn u04_header_probe_reads_the_painted_sorted_titles_and_only_drawn_cells() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    let probe = Probe::default();
    handle
        .update(&mut cx, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().list.borrow_mut().probe = Some(probe.clone());
        })
        .unwrap();
    frame(&mut cx, handle);
    let name = role(&probe, "header-0-name").expect("the custom header lost its label role");
    assert_eq!(
        name.get("text").and_then(Json::as_str),
        Some("Name \u{25b4}")
    );
    assert_eq!(
        role(&probe, "header-0-size")
            .unwrap()
            .get("text")
            .and_then(Json::as_str),
        Some("Size")
    );
    handle
        .update(&mut cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            let bounds = list.header.drawn.bounds;
            let rect = name.get("rect").and_then(Json::as_array).unwrap();
            let n = |at: usize| rect[at].as_f64().unwrap() as f32;
            assert!(n(0) >= f32::from(bounds.left()));
            assert!(n(1) >= f32::from(bounds.top()));
            assert!(n(2) > 0. && n(3) > 0.);
            assert!(n(0) + n(2) <= f32::from(bounds.right()));
            assert!(n(1) + n(3) <= f32::from(bounds.bottom()));
        })
        .unwrap();
    let probe = Probe::default();
    handle
        .update(&mut cx, |shell, _, _| {
            shell.launch.settings.data = flea::jsondoc::parse(r#"{"columns":["size"]}"#).unwrap();
            let host = shell.panes[0].as_mut().unwrap();
            host.nav.order.desc = true;
            host.list.borrow_mut().probe = Some(probe.clone());
        })
        .unwrap();
    frame(&mut cx, handle);
    assert_eq!(
        role(&probe, "header-0-name")
            .unwrap()
            .get("text")
            .and_then(Json::as_str),
        Some("Name \u{25be}")
    );
    for hidden in ["mode", "modified", "kind"] {
        assert!(role(&probe, &format!("header-0-{hidden}")).is_none());
    }
}

#[test]
fn dual_header_name_cell_is_the_row_name_cell() {
    // The header reads the rows' own name cell (the Dual mark slot), whatever metrics a caller hands it.
    let metrics = Metrics::at(px(14.));
    let tokens = Tokens::measure(metrics.pad, 19., metrics.gap, 7., 13.);
    let layout = columns::resolve(
        400.,
        &tokens,
        Switched::default().for_dual(true),
        StoredWidths::default(),
    );
    assert_eq!(
        HeaderState::title_cell(&layout, &metrics, None, true),
        Some(layout.name),
        "the Dual header's Name sits over the rows' name column"
    );
    assert_eq!(
        HeaderState::title_cell(&layout, &metrics, None, false).map(|cell| cell.x),
        Some(metrics.pad),
        "the single List header keeps its board inset at the padding"
    );
}

#[test]
fn m9_dual_header_name_starts_at_the_label_column() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    for (view, dual) in [(View::List, false), (View::Dual, true)] {
        let probe = Probe::default();
        handle
            .update(&mut cx, |shell, _, _| {
                shell.view = view;
                shell.panes[0].as_ref().unwrap().list.borrow_mut().probe = Some(probe.clone());
            })
            .unwrap();
        frame(&mut cx, handle);
        let name = role(&probe, "header-0-name").expect("the custom header lost its label role");
        handle
            .update(&mut cx, |shell, _, _| {
                let list = shell.panes[0].as_ref().unwrap().list.borrow();
                assert_eq!(list.view.dual, dual);
                let bounds = list.header.drawn.bounds;
                let metrics = list.file_metrics();
                let at = if dual { metrics.label_x() } else { metrics.pad };
                let rect = name.get("rect").and_then(Json::as_array).unwrap();
                let x = rect[0].as_f64().unwrap() as f32;
                assert!(
                    (x - (f32::from(bounds.left()) + at)).abs() < 1.,
                    "header name at x {x}, expected {} (dual {dual})",
                    f32::from(bounds.left()) + at
                );
            })
            .unwrap();
    }
}
