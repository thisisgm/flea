// A mirrored window keeps an unmirrored row's order, so a click lands on the child painted there.
use super::unmirrored;
use gpui::{
    div, point, prelude::*, px, AnyElement, Context, Modifiers, MouseButton, TestAppContext, Window,
};
use std::{cell::RefCell, rc::Rc};

type Log = Rc<RefCell<Vec<&'static str>>>;

// Two targets in a 50 px row at the logical left, wrapped in `unmirrored` when `kept`.
struct Row(Log, bool);

impl Render for Row {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let target = |name: &'static str| {
            let log = self.0.clone();
            div()
                .id(name)
                .w(px(20.))
                .h(px(20.))
                .on_mouse_down(MouseButton::Left, move |_, _, _| {
                    log.borrow_mut().push(name)
                })
        };
        let row = div()
            .flex()
            .w(px(50.))
            .h(px(20.))
            .child(target("a"))
            .child(div().w(px(10.)))
            .child(target("b"));
        let row: AnyElement = if self.1 {
            unmirrored(row).into_any_element()
        } else {
            row.into_any_element()
        };
        div().size_full().child(row)
    }
}

fn clicked(cx: &mut TestAppContext, kept: bool) -> Vec<&'static str> {
    let log = Log::default();
    let shared = log.clone();
    let (_, cx) = cx.add_window_view(move |_, _| Row(shared, kept));
    let width = cx.update(|window, _| {
        window.set_layout_mirrored(true);
        window.viewport_size().width
    });
    cx.run_until_parked();
    cx.simulate_click(point(width - px(40.), px(10.)), Modifiers::default());
    log.take()
}

#[gpui::test]
fn i50_rtl_an_unmirrored_row_keeps_its_order_under_the_pointer(cx: &mut TestAppContext) {
    // Kept, the row moves whole and a paints at its mirrored left edge; reflected, b paints there.
    assert_eq!(clicked(cx, true), ["a"]);
    assert_eq!(clicked(cx, false), ["b"]);
}
