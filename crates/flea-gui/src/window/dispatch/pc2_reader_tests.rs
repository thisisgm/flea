//! HTML capture reads the production Root's painted frame and the viewport's live lane.
use super::{previewtypes_tests::file, test_shell};
use crate::{
    g2::{pv1::doc::View as LookDoc, pv2::html::HtmlView},
    ipc::rows::colour,
    state::Shell,
    window::{
        selection_preview::{docs::Doc, Mounted},
        u07_tests::read,
    },
};
use flea::jsondoc::{self, Json};
use gpui::{point, px, Entity, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext};
use std::{sync::Arc, time::Duration};

#[path = "pc2_reader_contract_tests.rs"]
mod contracts;

fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(300));
        frame(cx);
    }
}

fn state(shell: &Entity<Shell>, cx: &mut VisualTestContext, column: bool) -> Json {
    jsondoc::parse(&read(
        shell,
        cx,
        "htmlState",
        &[if column { "true" } else { "false" }],
    ))
    .unwrap()
}

fn picture(view: &Entity<HtmlView>, cx: &mut VisualTestContext, height: u32) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.image = Some((
                1,
                Arc::new(
                    gpui::RenderImage::from_bgra(100, height, vec![255; 400 * height as usize])
                        .unwrap(),
                ),
                (100, height),
            ));
            view.doc.picture = flea::ui_logic_pv2::html::Picture::Ready;
            view.doc.has_drawn = true;
            view.doc.pending = false;
            cx.notify();
        })
    });
    settle(cx);
}

fn lane(state: &Json, shown: bool, visible: bool) -> &Json {
    let lane = state.get("lane").expect("capture includes lane");
    assert_eq!(
        lane.get("shown").and_then(Json::as_bool),
        Some(shown),
        "{state:?}"
    );
    assert_eq!(
        lane.get("visible").and_then(Json::as_bool),
        Some(visible),
        "{state:?}"
    );
    let rect = lane.get("rect").and_then(Json::as_str).unwrap();
    assert_eq!(rect.split_whitespace().count(), 4);
    lane
}

fn wheel(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(-80.))),
        ..Default::default()
    });
    frame(cx);
}

#[gpui::test]
fn pc2_reader_html_quick_look_frame_and_lane_follow_mounted_input(cx: &mut TestAppContext) {
    let (shell, cx) = test_shell::open_root(cx, "default");
    test_shell::land(
        &shell,
        cx,
        "/files",
        vec![file("page.html", "text-html", "HTML", 0o100644)],
        1,
    );
    assert_eq!(state(&shell, cx, false), Json::Obj(vec![]));
    cx.simulate_keystrokes("space");
    settle(cx);
    let view = shell.read_with(cx, |shell, cx| {
        let look = shell.quick_look.as_ref().unwrap().view.read(cx);
        let LookDoc::Html(view) = &look.subject().unwrap().body.doc().unwrap().view else {
            panic!("HTML mounted")
        };
        view.clone()
    });
    assert_eq!(state(&shell, cx, false).get("lane"), Some(&Json::Null));
    picture(&view, cx, 2000);
    let observed = state(&shell, cx, false);
    assert_eq!(observed.get("hairline").and_then(Json::as_f64), Some(1.));
    let (ring, canvas, canvas_ink) = cx.update(|window, cx| {
        let look = shell.read(cx).quick_look.as_ref().unwrap().view.read(cx);
        assert!(look.subject().unwrap().body.framed(cx));
        let style = look.look(
            &window.text_style(),
            cx.try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default(),
        );
        (
            colour(style.muted_frame),
            colour(style.surface),
            style.surface,
        )
    });
    assert_eq!(
        observed.get("ringColor").and_then(Json::as_str),
        Some(ring.as_str())
    );
    assert_eq!(
        observed.get("canvasColor").and_then(Json::as_str),
        Some(canvas.as_str())
    );
    let card = cx.debug_bounds("pv1-card").expect("HTML card painted");
    let (scale, quads) = cx.update(|window, _| (window.scale_factor(), window.painted_quads()));
    let painted = quads
        .iter()
        .find(|q| q.bounds == card.scale(scale) && q.border_widths.top.0 > 0.)
        .unwrap();
    assert!(quads.iter().any(|quad| quad.bounds == card.scale(scale)
        && quad.background == gpui::fill(card, canvas_ink).background));
    assert_eq!(
        observed.get("hairline").and_then(Json::as_f64),
        Some(f64::from(painted.border_widths.top.0 / scale))
    );
    assert_eq!(
        observed.get("ringColor").and_then(Json::as_str),
        Some(colour(painted.border_color).as_str())
    );
    cx.update(|window, cx| {
        let look = shell.read(cx).quick_look.as_ref().unwrap().view.clone();
        look.update(cx, |look, cx| {
            let mut style = look.look(
                &window.text_style(),
                cx.try_global::<crate::palette::Theme>()
                    .copied()
                    .unwrap_or_default(),
            );
            let theme = crate::presentation::frame_gate::theme();
            style.muted_frame = theme.muted_frame;
            style.surface = theme.surface;
            look.set_look(style, cx);
        });
    });
    frame(cx);
    let restyled = state(&shell, cx, false);
    assert_ne!(restyled.get("ringColor"), observed.get("ringColor"));
    assert_ne!(restyled.get("canvasColor"), observed.get("canvasColor"));
    lane(&observed, false, true);
    let bounds = cx.read(|cx| view.read(cx).bounds.unwrap());
    wheel(cx, bounds.center());
    let moved = state(&shell, cx, false);
    lane(&moved, true, true);
    assert!(cx.read(|cx| view.read(cx).scroll_y) > 0.);
    settle(cx);
    let idle = state(&shell, cx, false);
    cx.simulate_mouse_move(
        point(bounds.right() - px(2.), bounds.bottom() - px(2.)),
        None,
        Default::default(),
    );
    frame(cx);
    lane(&state(&shell, cx, false), false, true);
    let rect: Vec<f32> = lane(&idle, false, true)
        .get("rect")
        .unwrap()
        .as_str()
        .unwrap()
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    let centre = point(px(rect[0] + rect[2] / 2.), px(rect[1] + rect[3] / 2.));
    cx.simulate_mouse_move(centre, None, Default::default());
    frame(cx);
    lane(&state(&shell, cx, false), true, true);
    cx.simulate_mouse_down(centre, gpui::MouseButton::Left, Default::default());
    frame(cx);
    lane(&state(&shell, cx, false), true, true);
    let outside_lane = point(bounds.center().x, centre.y);
    cx.simulate_mouse_move(
        outside_lane,
        Some(gpui::MouseButton::Left),
        Default::default(),
    );
    settle(cx);
    lane(&state(&shell, cx, false), true, true);
    cx.simulate_mouse_up(outside_lane, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(point(px(1.), px(1.)), None, Default::default());
    settle(cx);
    lane(&state(&shell, cx, false), false, true);
    picture(&view, cx, 1);
    lane(&state(&shell, cx, false), false, false);
    cx.simulate_keystrokes("escape");
    frame(cx);
    assert_eq!(state(&shell, cx, false), Json::Obj(vec![]));
}

#[gpui::test]
fn pc2_reader_html_column_source_and_quick_look_choice(cx: &mut TestAppContext) {
    let (shell, cx) = test_shell::open_root_with(
        cx,
        r#""keys":"default","view":"columns","preview":{"column":true}"#,
        None,
    );
    test_shell::land(
        &shell,
        cx,
        "/files",
        vec![file("page.html", "text-html", "HTML", 0o100644)],
        1,
    );
    super::previewtypes_tests::answer(
        &shell,
        cx,
        r#"{"t":"fsinfo","path":"/files","fs":"tmpfs","dev":1,"total":100,"free":90,"class":"local"}"#,
    );
    settle(cx);
    let lines = test_shell::sent();
    let meta = super::previewtypes_tests::ask(&lines, "meta");
    let token = super::previewtypes_tests::field(meta, "token");
    super::previewtypes_tests::answer(
        &shell,
        cx,
        &format!(r#"{{"t":"meta","token":{token},"path":"/files/page.html","lines":300}}"#),
    );
    settle(cx);
    let view = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).unwrap().read(cx);
        let mounted = &preview
            .pending
            .as_ref()
            .or(preview.drawn.as_ref())
            .unwrap()
            .mounted;
        let Mounted::Doc(Doc::Html(view)) = mounted else {
            panic!("column HTML mounted")
        };
        view.clone()
    });
    picture(&view, cx, 2000);
    let column = state(&shell, cx, true);
    assert_eq!(column.get("column").and_then(Json::as_bool), Some(true));
    assert_eq!(column.get("hairline").and_then(Json::as_f64), Some(1.));
    assert_eq!(
        column.get("canvasColor").and_then(Json::as_str),
        Some("#00000000")
    );
    assert_eq!(column.get("close"), Some(&Json::Null));
    lane(&column, false, true);
    assert_eq!(state(&shell, cx, false), Json::Obj(vec![]));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let generation = view.set_path("/files/page.html", 600, true, cx);
            view.set_source_text(generation, Some("line\n".repeat(300)), false, cx);
            view.set_view(flea::ui_logic_pv2::html::View::Source, cx);
        })
    });
    settle(cx);
    let lines = test_shell::sent();
    let asked = super::previewtypes_tests::ask(&lines, "code");
    let body = (0..300)
        .map(|n| format!(r#"[0,"line {n}"]"#))
        .collect::<Vec<_>>()
        .join(",");
    super::previewtypes_tests::answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"plain","first":{},"total":300,"lines":[{body}]}}"#,
            super::previewtypes_tests::field(asked, "token"),
            super::previewtypes_tests::field(asked, "first")
        ),
    );
    settle(cx);
    let source = state(&shell, cx, true);
    assert_eq!(source.get("view").and_then(Json::as_str), Some("source"));
    lane(&source, false, true);
    let bounds = cx.read(|cx| view.read(cx).bounds.unwrap());
    wheel(cx, bounds.center());
    lane(&state(&shell, cx, true), true, true);
    cx.simulate_keystrokes("space");
    settle(cx);
    let look = state(&shell, cx, false);
    assert_eq!(look.get("column").and_then(Json::as_bool), Some(false));
    assert_eq!(look.get("hairline").and_then(Json::as_f64), Some(1.));
    assert_ne!(look.get("canvasColor"), column.get("canvasColor"));
    assert_eq!(
        state(&shell, cx, true)
            .get("column")
            .and_then(Json::as_bool),
        Some(true)
    );
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(state(&shell, cx, false), Json::Obj(vec![]));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_view(crate::window::startup::View::List, window, cx)
        })
    });
    settle(cx);
    assert_eq!(state(&shell, cx, true), Json::Obj(vec![]));
}
