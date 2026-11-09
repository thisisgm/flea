//! EY-348: a long fact value wraps inside the card in a small window, and the names keep one column.
use super::{Block, Card, DialogShell, Fact, StandIn, FACT_LINE};
use crate::g2::open_with_properties::tests::{english, permissions};
use crate::list::{Palette, Typeface};
use crate::palette::Theme;
use gpui::{prelude::*, px, size, Context, Render, TestAppContext, VisualTestContext, Window};

struct Probe(Card, Typeface);

impl Render for Probe {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::inherited(&window.text_style(), Theme::default());
        StandIn.present(&self.0, &palette, &self.1, 0., 650.)
    }
}

#[gpui::test]
fn ey348_a_long_path_wraps_inside_the_card(cx: &mut TestAppContext) {
    let text = |name: &str, value: &str| Fact::Text {
        name: name.into(),
        value: value.into(),
        tail: String::new(),
    };
    let mut card = permissions().card(&english());
    card.body = vec![Block::Facts(vec![
        text(
            "Path",
            "/home/flea-sandbox/fn.yJ7hAvvD/arms/pf.dcw-rest.pf1-card-body-scroll.current.base/fixture/work/A",
        ),
        text("Permissions", "0755"),
    ])];
    card.buttons.clear();
    let face = Typeface::omarchy_default();
    let handle = cx.add_window(|_, _| Probe(card, face));
    cx.simulate_window_resize(handle.into(), size(px(650.), px(320.)));
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.run_until_parked();
    let frame = visual.debug_bounds("op3-card").expect("the card is drawn");
    let path = visual
        .debug_bounds("op3-fact-0")
        .expect("the Path value is drawn");
    let mode = visual
        .debug_bounds("op3-fact-1")
        .expect("the Permissions value is drawn");
    assert!(
        path.right() <= frame.right(),
        "the Path value stays inside the card: {path:?} in {frame:?}"
    );
    let line = px(FACT_LINE * f32::from(Typeface::omarchy_default().body) / 14.);
    assert!(
        path.size.height >= line * 2.,
        "the Path value wraps: {path:?}"
    );
    assert_eq!(path.origin.x, mode.origin.x, "the values keep one column");
    assert!(
        mode.origin.y >= path.bottom(),
        "the next fact starts under the wrapped path"
    );
}

#[gpui::test]
fn ey348_wrapped_facts_keep_the_whole_card_inside_a_short_window(cx: &mut TestAppContext) {
    let facts = [
        ("Path", "/home/flea-sandbox/fn.LctF5vQj/arms/fcl.ey348.props-long-path.current.gpui/fixture/work/A"),
        ("Kind", "Folder"),
        ("Size", "243 B (243 bytes)"),
        ("Contains", "10 items"),
        ("Modified", "2026-09-29 18:27"),
        ("Permissions", "0755"),
        ("Owner", "gm (uid 1000)"),
        ("Group", "gid 1000"),
    ].into_iter().map(|(name, value)| Fact::Text {
        name: name.into(), value: value.into(), tail: String::new(),
    }).collect();
    let card = Card {
        language: "en".into(),
        title: "Properties".into(),
        esc: "esc".into(),
        width: 550.,
        bleed: false,
        body: vec![Block::Facts(facts)],
        buttons: vec![super::Button {
            label: "Close".into(),
            primary: true,
            focused: true,
            enabled: true,
        }],
    };
    let face = Typeface {
        base: px(14.),
        body: px(14.),
        body_small: px(13.),
        caption: px(12.),
        ..Typeface::omarchy_default()
    };
    let handle = cx.add_window(|_, _| Probe(card, face));
    cx.simulate_window_resize(handle.into(), size(px(650.), px(320.)));
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.run_until_parked();
    let frame = visual
        .debug_bounds("op3-card")
        .expect("Properties is drawn");
    assert!(
        frame.top() >= px(super::GAP),
        "the full title stays inside the window: {frame:?}"
    );
    assert!(
        frame.bottom() <= px(320. - super::GAP),
        "the full Close button stays inside the window: {frame:?}"
    );
    let body = visual
        .debug_bounds("op3-facts-body")
        .expect("the fact body is drawn");
    let close = visual.debug_bounds("op3-button-0").expect("Close is drawn");
    assert!(
        body.bottom() < close.top(),
        "the body scrolls above the pinned Close button"
    );
    visual.simulate_mouse_move(body.center(), None, gpui::Modifiers::none());
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: body.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-500.))),
        ..Default::default()
    });
    visual.run_until_parked();
    let group = visual
        .debug_bounds("op3-fact-7")
        .expect("the last fact can be reached");
    assert!(
        group.bottom() <= body.bottom() + px(1.) && group.top() >= body.top() - px(1.),
        "scrolling reveals the last fact: {group:?} inside {body:?}"
    );
    assert_eq!(
        visual.debug_bounds("op3-button-0"),
        Some(close),
        "scrolling the body keeps Close pinned"
    );
}
