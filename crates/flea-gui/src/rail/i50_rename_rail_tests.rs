//! The rail field only attaches while its own row holds the keyboard.
use super::{draw, TestAppContext, VisualTestContext};
use crate::g2::rail_places::producer::Producer;
use flea::ui_logic_rail_places::{
    order::{Heading, RailSection},
    rail::{Group, Kind, RailEntry},
};
use gpui::{div, prelude::*, AppContext, Context, Entity, FocusHandle, Window};
use std::{path::PathBuf, sync::Arc};

struct RailFixture {
    producer: Entity<Producer>,
    focus: FocusHandle,
    other: FocusHandle,
}

impl Render for RailFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .child(
                div()
                    .w(gpui::px(192.))
                    .h_full()
                    .track_focus(&self.focus)
                    .child(self.producer.clone()),
            )
            .child(div().id("other-control").track_focus(&self.other))
    }
}

fn ready(cx: &mut TestAppContext) -> (Entity<RailFixture>, &mut VisualTestContext) {
    let result = cx.add_window_view(|window, cx| {
        let focus = cx.focus_handle();
        let other = cx.focus_handle();
        let producer = cx.new(|cx| {
            Producer::from_sections(
                Arc::new(crate::test_catalogs::english()),
                vec![RailSection {
                    heading: Heading::Plugin("Network".into()),
                    rows: vec![RailEntry {
                        group: Group::Network,
                        kind: Kind::Share,
                        label: "NAS".into(),
                        ..Default::default()
                    }],
                }],
                PathBuf::from("/unused-test-backend"),
                cx,
            )
        });
        producer.update(cx, |p, cx| {
            p.model.borrow_mut().focused = true;
            assert!(p.start_rename(0, cx));
        });
        window.focus(&focus, cx);
        RailFixture {
            producer,
            focus,
            other,
        }
    });
    draw(result.1);
    result
}

#[gpui::test]
fn i50_rename_rail_handler_attachment(cx: &mut TestAppContext) {
    let (fixture, cx) = ready(cx);
    cx.simulate_input("倉庫");
    fixture.read_with(cx, |f, cx| {
        let p = f.producer.read(cx);
        assert_eq!(p.rename_text(), "倉庫");
        assert!(p.laid.borrow().row(0).unwrap().editor.is_some());
    });
    cx.update(|w, _| w.remove_window());
}

#[gpui::test]
fn i50_rename_rail_blur(cx: &mut TestAppContext) {
    let (fixture, cx) = ready(cx);
    cx.simulate_input("draft");
    cx.update(|w, cx| {
        fixture.update(cx, |f, cx| {
            f.producer
                .update(cx, |p, _| p.model.borrow_mut().focused = false);
            w.focus(&f.other, cx);
        })
    });
    draw(cx);
    cx.simulate_input("stale");
    fixture.read_with(cx, |f, cx| {
        assert_eq!(
            f.producer.read(cx).rename_text(),
            "draft",
            "a rail that lost keyboard focus cannot attach its editor to another control"
        );
    });
    cx.update(|w, _| w.remove_window());
}

#[gpui::test]
fn i50_rename_rail_offscreen_field_has_no_handler(cx: &mut TestAppContext) {
    let (fixture, cx) = ready(cx);
    cx.simulate_resize(gpui::size(gpui::px(192.), gpui::px(1.)));
    draw(cx);
    cx.simulate_input("hidden");
    fixture.read_with(cx, |f, cx| {
        assert_eq!(
            f.producer.read(cx).rename_text(),
            "NAS",
            "an offscreen rail name cannot receive platform text"
        );
    });
    cx.update(|w, _| w.remove_window());
}
