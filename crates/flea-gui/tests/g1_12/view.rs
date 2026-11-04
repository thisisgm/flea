//! The path surface in a window: the strip's first-tap parent, the field's keys, Tab's peek and both ways to choose a row.
use super::{
    inputs,
    jump::{answer, Folders},
    HOME,
};
use flea_gui::path::{
    paint::Size,
    peek_line,
    render::pane_strip,
    suggestions::{Ranker, Unranked},
    view::{Inputs, Out, PathView},
    Answer, Peeked,
};
use gpui::{
    div, point, prelude::*, px, size, Context, Entity, Modifiers, MouseButton, MouseDownEvent,
    ScrollDelta, ScrollWheelEvent, Subscription, TestAppContext, VisualTestContext, Window,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// The composition in small: the chrome's 26 px path slot over two 27 px pane strips.
pub struct Host {
    pub path: Entity<PathView>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(
                div()
                    .absolute()
                    .left(px(0.))
                    .top(px(0.))
                    .w(px(788.))
                    .h(px(26.))
                    .child(self.path.clone()),
            )
            .child(
                div()
                    .absolute()
                    .left(px(0.))
                    .top(px(26.))
                    .w(px(400.))
                    .h(px(27.))
                    .child(pane_strip(&self.path, 0, cx)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(401.))
                    .top(px(26.))
                    .w(px(387.))
                    .h(px(27.))
                    .child(pane_strip(&self.path, 1, cx)),
            )
    }
}

pub struct Bar<'a> {
    pub view: Entity<PathView>,
    pub cx: &'a mut VisualTestContext,
    pub said: Rc<RefCell<Vec<Out>>>,
    pub _events: Subscription,
}

impl Bar<'_> {
    pub fn sync(&mut self, inputs: Inputs) {
        self.view.update(self.cx, |view, cx| view.sync(inputs, cx));
        self.cx.run_until_parked();
    }

    pub fn advance(&mut self, size: Size) -> f32 {
        self.view.read_with(self.cx, |view, _| {
            f32::from(view.paint.borrow().measured(size))
        })
    }

    pub fn press(&mut self, x: f32, y: f32, taps: usize) {
        self.cx.simulate_event(MouseDownEvent {
            position: point(px(x), px(y)),
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count: taps,
            first_mouse: false,
        });
        self.cx
            .simulate_mouse_up(point(px(x), px(y)), MouseButton::Left, Modifiers::none());
    }

    pub fn take(&mut self) -> Vec<Out> {
        std::mem::take(&mut *self.said.borrow_mut())
    }

    pub fn update<R>(
        &mut self,
        f: impl FnOnce(&mut PathView, &mut Window, &mut Context<PathView>) -> R,
    ) -> R {
        let view = self.view.clone();
        let result = self
            .cx
            .update(|window, cx| view.update(cx, |view, cx| f(view, window, cx)));
        self.cx.run_until_parked();
        result
    }

    pub fn read<R>(&mut self, f: impl FnOnce(&PathView) -> R) -> R {
        self.view.read_with(self.cx, |view, _| f(view))
    }
}

pub fn bar(cx: &mut TestAppContext, ranker: Box<dyn Ranker>, inputs: Inputs) -> Bar<'_> {
    let (host, cx) = cx.add_window_view(|_, cx| Host {
        path: cx.new(|cx| PathView::new(Arc::new(crate::test_catalogs::english()), ranker, cx)),
    });
    cx.simulate_resize(size(px(1100.), px(360.)));
    let view = host.read_with(cx, |host, _| host.path.clone());
    let said = Rc::new(RefCell::new(Vec::new()));
    let log = said.clone();
    let events = cx.update(|_, cx| {
        cx.subscribe(&view, move |_, out: &Out, _| {
            log.borrow_mut().push(out.clone())
        })
    });
    let mut bar = Bar {
        view,
        cx,
        said,
        _events: events,
    };
    bar.sync(inputs);
    bar
}

fn open(side: usize, path: &str) -> Out {
    Out::Open {
        side,
        path: path.into(),
    }
}

pub fn ranked<'a>(cx: &'a mut TestAppContext, query: &str) -> Bar<'a> {
    let mut bar = bar(cx, Box::new(Folders::default()), inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    let asks = bar.take();
    assert!(
        matches!(
            asks.as_slice(),
            [Out::Send { side: 0, .. }, Out::ReadHistory]
        ),
        "{asks:?}"
    );
    let zoxide = [
        "/fx/v/work/Media",
        "/srv/omarchy",
        "/home/gm/Documents/Notes",
        "/home/gm/Music",
    ];
    bar.update(|view, window, cx| view.receive(Answer::Jumped(answer(1, &zoxide)), window, cx));
    bar.update(|view, _, cx| view.history_read(Vec::new(), cx));
    let whole = bar.take();
    assert!(
        matches!(whole.as_slice(), [Out::Send { side: 0, .. }]),
        "{whole:?}"
    );
    bar.update(|view, window, cx| view.receive(Answer::Jumped(answer(2, &zoxide)), window, cx));
    bar.cx.simulate_input(query);
    bar
}

#[path = "view_interaction_tests.rs"]
mod interactions;
