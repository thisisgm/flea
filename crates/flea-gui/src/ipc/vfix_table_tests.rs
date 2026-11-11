//! The table reader reports the mounted Markdown table and follows the real horizontal wheel route.
use super::readers3_tests::{open, read};
use crate::g2::{
    doc_preview::markdown::{Laid, Status},
    quick_look_overlay::doc::View,
};
use flea::{
    jsondoc::{self, Json},
    ui_logic_doc_preview::markdown::{
        markdown_layout::{FigureState, Geometry, Item},
        Block, Figure, FigureKind, Table,
    },
};
use gpui::{
    point, px, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, VisualTestContext,
};
use std::sync::Arc;

fn table(cx: &mut TestAppContext, handle: gpui::WindowHandle<crate::window::root::Root>) -> Json {
    let reply = read(cx, handle, "previewTable", &[]);
    assert_eq!(reply.code, 0, "{reply:?}");
    jsondoc::parse(&reply.stdout).unwrap()
}

#[gpui::test]
fn vf_readers_table_reports_painted_geometry_and_wheel_position(cx: &mut TestAppContext) {
    let handle = open(cx);
    let doc = crate::g2::doc_preview::render::without_markdown_threads(|| {
        super::reader_port_b_tests::document(cx, handle, "wide.md", "text-markdown")
    });
    let View::Markdown(view) = doc.view else {
        panic!("mounted Markdown");
    };
    view.update(cx, |view, cx| {
        view.doc.seq += 1;
        view.doc.status = Status::Ready;
        view.doc.shown = true;
        view.doc.complete = true;
        cx.notify();
    });
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    view.update(cx, |view, cx| {
        let mut parsed = view.doc.parsed.clone();
        parsed.blocks = Arc::new(vec![
            Block::Table(Table {
                head: vec!["Header".into()],
                rows: vec![vec!["Body".into()]],
                ..Default::default()
            }),
            Block::Figure(Figure {
                kind: FigureKind::Mermaid,
                source: "graph TD; A-->B".into(),
                display: true,
            }),
        ]);
        view.doc.take(parsed, true);
        view.doc.status = Status::Ready;
        view.doc.shown = true;
        Arc::make_mut(&mut view.doc.assets)
            .figures
            .insert(0, FigureState::Waiting);
        view.doc.width = (view.frame.0 - 2. * view.tokens.inset_x()).floor();
        let width = (view.doc.width - 5.).max(20.);
        let items = vec![Item::Table {
            key: None,
            x: 0.,
            y: 0.,
            width,
            content: width + 500.,
            height: 60.,
            items: vec![
                Item::Rule {
                    x: 0.,
                    y: 27.,
                    width: width + 500.,
                    wash: true,
                },
                Item::Rule {
                    x: 0.,
                    y: 56.,
                    width: width + 500.,
                    wash: true,
                },
            ],
        }];
        view.doc.laid[0] = Some(Laid {
            geometry: Arc::new(Geometry {
                height: 60.,
                items,
                first_baseline: None,
            }),
            generation: view.doc.generation,
            version: view.doc.version,
        });
        view.doc.retop(&view.tokens.metrics());
        cx.notify();
    });
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    let before = table(cx, handle);
    let figures = read(cx, handle, "previewFigures", &[]);
    assert_eq!(figures.code, 0, "{figures:?}");
    assert_eq!(figures.stdout.trim(), "1=working");
    view.update(cx, |view, cx| {
        Arc::make_mut(&mut view.doc.assets)
            .figures
            .insert(0, FigureState::Failed);
        cx.notify();
    });
    let figures = read(cx, handle, "previewFigures", &[]);
    assert_eq!(figures.code, 0, "{figures:?}");
    assert_eq!(figures.stdout.trim(), "1=failed");
    assert_eq!(before, table(cx, handle));
    assert_eq!(before.get("scrollX").and_then(Json::as_f64), Some(0.));
    let rect = |key| {
        before
            .get(key)
            .and_then(Json::as_str)
            .unwrap()
            .split_whitespace()
            .map(|n| n.parse::<f32>().unwrap())
            .collect::<Vec<_>>()
    };
    let frame = rect("view");
    let row = rect("row");
    assert_eq!(frame.len(), 4);
    assert_eq!(row.len(), 4);
    assert!(row[2] > frame[2]);
    VisualTestContext::from_window(handle.into(), cx).simulate_event(ScrollWheelEvent {
        position: point(px(frame[0] + 5.), px(frame[1] + 5.)),
        delta: ScrollDelta::Lines(point(-3., 0.)),
        touch_phase: TouchPhase::Moved,
        modifiers: Default::default(),
    });
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    let after = table(cx, handle);
    assert!(after.get("scrollX").and_then(Json::as_f64).unwrap() > 0.);
    assert_eq!(before.get("view"), after.get("view"));
    assert_ne!(before.get("row"), after.get("row"));
}
