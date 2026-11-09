//! ui/SettingsPanel.qml and ui/KeymapSheet.qml grounds: nothing beneath sees hover, a press or the wheel, a left or right release on the ground closes, and a key never does.
use super::seams::Request;
use super::sheet::{Sheet, Sources};
use super::sheet_view::SheetView;
use super::tests::{english, panel};
use super::view::SettingsView;
use gpui::{
    div, point, prelude::*, px, AnyView, Context, FocusHandle, KeyUpEvent, Keystroke, Modifiers,
    MouseButton, Pixels, Point, Render, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase,
    VisualTestContext, Window,
};
use std::cell::RefCell;
use std::rc::Rc;

type Log = Rc<RefCell<Vec<&'static str>>>;

// A window-sized listing that logs what reaches it, with the overlay drawn over it.
struct Host {
    log: Log,
    overlay: AnyView,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (down, right, wheel, moved) = (
            Rc::clone(&self.log),
            Rc::clone(&self.log),
            Rc::clone(&self.log),
            Rc::clone(&self.log),
        );
        div()
            .size_full()
            .relative()
            .child(
                div()
                    .id("listing")
                    .absolute()
                    .size_full()
                    .on_mouse_down(MouseButton::Left, move |_, _, _| {
                        down.borrow_mut().push("left")
                    })
                    .on_mouse_down(MouseButton::Right, move |_, _, _| {
                        right.borrow_mut().push("right")
                    })
                    .on_scroll_wheel(move |_, _, _| wheel.borrow_mut().push("wheel"))
                    .on_mouse_move(move |_, _, _| moved.borrow_mut().push("hover")),
            )
            .child(div().absolute().size_full().child(self.overlay.clone()))
    }
}

fn wheel(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(40.))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
    });
}

// At a point: hover and the wheel, then a press and release of each button; the requests seen after each press and each release.
fn probe(
    cx: &mut VisualTestContext,
    at: Point<Pixels>,
    requests: &Rc<RefCell<Vec<Request>>>,
) -> [usize; 4] {
    let none = Modifiers::default();
    cx.simulate_mouse_move(at, None, none);
    wheel(cx, at);
    let mut seen = [0; 4];
    for (step, button) in [MouseButton::Right, MouseButton::Left]
        .into_iter()
        .enumerate()
    {
        cx.simulate_mouse_down(at, button, none);
        seen[step * 2] = requests.borrow().len();
        cx.simulate_mouse_up(at, button, none);
        seen[step * 2 + 1] = requests.borrow().len();
    }
    seen
}

// A whole key press, down then up: simulate_keystrokes alone never sends the release.
fn press(cx: &mut VisualTestContext, key: &str) {
    cx.simulate_keystrokes(key);
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse(key).expect("key parses"),
    });
}

fn focused(cx: &mut VisualTestContext, focus: FocusHandle) {
    cx.update(|window, cx| {
        window.focus(&focus, cx);
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

type Hosted<'a, V> = (
    gpui::Entity<V>,
    Log,
    Rc<RefCell<Vec<Request>>>,
    &'a mut VisualTestContext,
);

fn hosted<V: Render + gpui::EventEmitter<Request>>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Context<V>) -> V,
) -> Hosted<'_, V> {
    let log: Log = Rc::default();
    let requests: Rc<RefCell<Vec<Request>>> = Rc::default();
    let (seen, sink) = (Rc::clone(&log), Rc::clone(&requests));
    let built: Rc<RefCell<Option<gpui::Entity<V>>>> = Rc::default();
    let keep = Rc::clone(&built);
    let (_, cx) = cx.add_window_view(move |_, cx| {
        let overlay = cx.new(build);
        cx.subscribe(&overlay, move |_, _, request: &Request, _| {
            sink.borrow_mut().push(request.clone())
        })
        .detach();
        *keep.borrow_mut() = Some(overlay.clone());
        Host {
            log: seen,
            overlay: overlay.into(),
        }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let overlay = built.borrow_mut().take().expect("overlay built");
    (overlay, log, requests, cx)
}

fn centre(cx: &mut VisualTestContext) -> Point<Pixels> {
    cx.update(|window, _| {
        let size = window.viewport_size();
        point(size.width / 2., size.height / 2.)
    })
}

#[gpui::test]
fn the_settings_ground_sinks_everything_and_closes_on_either_release(cx: &mut TestAppContext) {
    let (_, log, requests, cx) = hosted(cx, |cx| SettingsView::new(panel("{}"), cx));
    assert_eq!(
        probe(cx, point(px(3.), px(3.)), &requests),
        [0, 1, 1, 2],
        "each button closes on its release, never on its press"
    );
    assert_eq!(*requests.borrow(), [Request::Close, Request::Close]);
    requests.borrow_mut().clear();
    let at = centre(cx);
    probe(cx, at, &requests);
    assert!(
        !requests.borrow().contains(&Request::Close),
        "a press inside the card never closes"
    );
    assert_eq!(
        *log.borrow(),
        Vec::<&str>::new(),
        "the window beneath sees nothing"
    );
}

#[gpui::test]
fn the_sheet_ground_sinks_everything_and_closes_on_either_release(cx: &mut TestAppContext) {
    let (_, log, requests, cx) = hosted(cx, |cx| {
        SheetView::new(Sheet::new("default", english(), Sources::default()), cx)
    });
    assert_eq!(
        probe(cx, point(px(3.), px(3.)), &requests),
        [0, 1, 1, 2],
        "each button closes on its release, never on its press"
    );
    requests.borrow_mut().clear();
    let at = centre(cx);
    probe(cx, at, &requests);
    assert_eq!(
        *requests.borrow(),
        Vec::<Request>::new(),
        "a press inside the card never closes"
    );
    assert_eq!(
        *log.borrow(),
        Vec::<&str>::new(),
        "the window beneath sees nothing"
    );
}

// The focused ground turns an Enter or Space release into a click; v0.3.12 closes on a pointer click only.
#[gpui::test]
fn space_and_enter_on_a_settings_row_act_and_keep_the_panel_open(cx: &mut TestAppContext) {
    let (view, _, requests, cx) = hosted(cx, |cx| SettingsView::new(panel("{}"), cx));
    let focus = view.read_with(cx, |view, _| view.focus.clone());
    focused(cx, focus);
    press(cx, "j");
    press(cx, "space");
    let on = view.read_with(cx, |view, _| {
        view.panel
            .document()
            .get("rememberViewPerFolder")
            .and_then(flea::jsondoc::Json::as_bool)
    });
    assert_eq!(on, Some(true), "space toggles the focused check");
    press(cx, "enter");
    assert_eq!(
        *requests.borrow(),
        Vec::<Request>::new(),
        "no key closes the panel"
    );
}

#[gpui::test]
fn a_space_typed_into_the_sheet_query_keeps_the_sheet_open(cx: &mut TestAppContext) {
    let (view, _, requests, cx) = hosted(cx, |cx| {
        SheetView::new(Sheet::new("default", english(), Sources::default()), cx)
    });
    let focus = view.read_with(cx, |view, _| view.focus.clone());
    focused(cx, focus);
    for key in ["m", "space", "o"] {
        press(cx, key);
    }
    assert_eq!(
        view.read_with(cx, |view, _| view.sheet.query.clone()),
        "m o",
        "the space reaches the query"
    );
    assert!(
        !requests.borrow().contains(&Request::Close),
        "no key closes the sheet"
    );
}
