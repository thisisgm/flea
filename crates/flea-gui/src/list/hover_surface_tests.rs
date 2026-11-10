//! Crumbs and tabs retain their shipped plates under a stationary pointer in either setting.
use super::HoverWash;
use crate::{path::view::PathView, tabs::bar};
use gpui::{
    div, point, prelude::*, px, size, AppContext, Context, Entity, Modifiers, Render,
    TestAppContext, Window,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

struct Host {
    path: Entity<PathView>,
    frame: bar::Frame,
    labels: Rc<RefCell<bar::Labels>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .child(
                div()
                    .absolute()
                    .top(px(0.))
                    .w_full()
                    .h(px(27.))
                    .child(self.path.clone()),
            )
            .child(
                div()
                    .absolute()
                    .top(px(27.))
                    .w_full()
                    .h(px(27.))
                    .child(bar::StripElement::new(
                        self.frame.clone(),
                        self.labels.clone(),
                    )),
            )
    }
}

#[gpui::test]
fn highlight_hover_preserves_crumb_and_tab_plates(cx: &mut TestAppContext) {
    let (host, cx) = cx.add_window_view(|_, cx| {
        let path = cx.new(|cx| {
            let mut path = PathView::new(
                Arc::new(crate::test_catalogs::english()),
                Box::new(crate::path::suggestions::Unranked),
                cx,
            );
            path.inputs.paths[0] = "/files/docs".into();
            path
        });
        let mut frame = crate::tabs::Bar::new("hover-test").frame(
            &crate::tabs::TabState::default(),
            "/files/docs",
            "/home/test",
            "Home",
        );
        frame.tab_width = 160.;
        Host {
            path,
            frame,
            labels: Rc::new(RefCell::new(bar::Labels::new(
                crate::list::Typeface::omarchy_default(),
            ))),
        }
    });
    cx.simulate_resize(size(px(600.), px(200.)));
    cx.run_until_parked();
    let path = host.read_with(cx, |host, _| host.path.clone());
    let crumb_x = path.read_with(cx, |path, _| {
        let (left, width) = path.strips[0].extent(1).expect("the files crumb");
        path.paint.borrow().strips[0].unwrap().left() + px(left + width / 2.)
    });
    for (name, at, top, height) in [
        ("crumb", point(crumb_x, px(12.)), 0., 27.),
        ("tab", point(px(50.), px(39.)), 27., 26.),
    ] {
        cx.simulate_mouse_move(at, None, Modifiers::none());
        cx.run_until_parked();
        for on in [false, true, false] {
            cx.update(|window, cx| {
                cx.set_global(HoverWash(on));
                window.refresh();
                window.draw(cx).clear(cx);
                let scale = window.scale_factor();
                let washed = window.painted_quads().iter().any(|quad| {
                    (quad.bounds.top().0 / scale - top).abs() < 0.01
                        && (quad.bounds.size.height.0 / scale - height).abs() < 0.01
                        && quad
                            .background
                            .as_solid()
                            .is_some_and(|ink| (ink.a - crate::list::HOVER).abs() < 0.0001)
                });
                assert!(!washed, "{name}: Highlight on hover={on} adds no plate");
            });
        }
    }
}
