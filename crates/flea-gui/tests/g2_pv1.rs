//! pv1: Quick Look's host. #331 holds every pointer event on the overlay away from the listing behind it.
#[path = "../../../src/i18n/test_catalogs.rs"]
mod test_catalogs;
// test_catalogs reaches the catalog types through super.
use flea::i18n::{I18n, Paths};
use flea::ui_logic_pv1::{kinds::State, rect::Shape};
use flea_gui::g2::pv1::{Body, Event, QuickLook, Subject};
use gpui::{
    canvas, div, point, prelude::*, px, size, Context, DispatchPhase, Entity, Modifiers,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, ScrollDelta,
    ScrollWheelEvent, TestAppContext, TouchPhase, VisualTestContext, Window,
};
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::Arc};

#[path = "g2_pv1/boards.rs"]
mod boards;
#[path = "g2_pv1/click_through.rs"]
mod click_through;
#[path = "g2_pv1/column.rs"]
mod column;
#[path = "g2_pv1/controls.rs"]
mod controls;
#[path = "g2_pv1/decoded.rs"]
mod decoded;
#[path = "g2_pv1/lifecycle.rs"]
mod lifecycle;
#[path = "g2_pv1/marks.rs"]
mod marks;
#[path = "g2_pv1/pixels.rs"]
mod pixels;
#[path = "g2_pv1/scenes.rs"]
pub mod scenes;
#[path = "g2_pv1/scroll.rs"]
mod scroll;
#[path = "g2_pv1/svg.rs"]
pub mod svg;
#[path = "g2_pv1/svgfit_join.rs"]
mod svgfit_join;
#[path = "g2_pv1/symlink.rs"]
mod symlink;
#[path = "g2_pv1/watchdog.rs"]
mod watchdog;

/// What the listing behind the overlay saw: each counter is one thing #331 says must not move.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    pub cursor: u32,
    pub selection: u32,
    pub scroll: u32,
    pub hover: u32,
    pub menu: u32,
    pub raw: u32,
}

/// Which overlay sits over the listing: the pv1 host, or a copy of the v0.3.12 card with no ground that takes the event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arm {
    Fix,
    OracleCopy,
}

pub struct Root {
    seen: Rc<RefCell<Seen>>,
    look: Entity<QuickLook>,
    arm: Arm,
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tally = |field: fn(&mut Seen) -> &mut u32| {
            let seen = self.seen.clone();
            move || *field(&mut seen.borrow_mut()) += 1
        };
        let (cursor, menu, scroll) = (
            tally(|s| &mut s.cursor),
            tally(|s| &mut s.menu),
            tally(|s| &mut s.scroll),
        );
        let (select, hover, raw) = (
            tally(|s| &mut s.selection),
            tally(|s| &mut s.hover),
            tally(|s| &mut s.raw),
        );
        let raw = Rc::new(raw);
        // The listing: a hitbox-checked handler for each listing reaction, and an unchecked bubble listener under them.
        let listing = div()
            .id("listing")
            .absolute()
            .size_full()
            .on_mouse_down(MouseButton::Left, move |_, _, _| cursor())
            .on_mouse_down(MouseButton::Right, move |_, _, _| menu())
            .on_scroll_wheel(move |_, _, _| scroll())
            .on_click(move |_, _, _| select())
            .on_hover(move |hovered, _, _| {
                if *hovered {
                    hover()
                }
            })
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let raw = raw.clone();
                        window.on_mouse_event(move |_: &MouseDownEvent, phase, _, _| {
                            if phase == DispatchPhase::Bubble {
                                raw()
                            }
                        });
                    },
                )
                .size_full(),
            );
        let overlay = match self.arm {
            Arm::Fix => self.look.clone().into_any_element(),
            Arm::OracleCopy => {
                let card = self.look.read(cx).card(window.viewport_size());
                div()
                    .absolute()
                    .left(card.origin.x)
                    .top(card.origin.y)
                    .w(card.size.width)
                    .h(card.size.height)
                    .into_any_element()
            }
        };
        div().size_full().relative().child(listing).child(overlay)
    }
}

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// A JSON fixture from the repo, drawn as the text card draws it.
pub fn json() -> Subject {
    let text = std::fs::read_to_string(repo("ci/visual/schema.json")).unwrap();
    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let count = crate::test_catalogs::english().trn("preview.lines", lines.len() as i64);
    Subject {
        name: "schema.json".into(),
        icon: "text-x-script".into(),
        mode: 0o100644,
        state: State::Code,
        shape: Shape::unknown(State::Text),
        body: Body::Text { lines, count },
    }
}

/// A PNG fixture from the repo, with the pixels its own header names.
pub fn image() -> Subject {
    let path = repo("docs/images/columns.png");
    let head = std::fs::read(&path).unwrap();
    let be = |at: usize| f64::from(u32::from_be_bytes(head[at..at + 4].try_into().unwrap()));
    let (w, h) = (be(16), be(20));
    Subject {
        name: "columns.png".into(),
        icon: "image-x-generic".into(),
        mode: 0o100644,
        state: State::Image,
        shape: Shape {
            kind: State::Image,
            known: true,
            w,
            h,
            orient: 1,
        },
        body: Body::Image { path, w, h },
    }
}

pub fn can_not_show(i18n: &flea::i18n::I18n) -> Subject {
    Subject {
        name: "disk.img".into(),
        icon: "application-x-generic".into(),
        mode: 0o100644,
        state: State::Unsupported,
        shape: Shape::unknown(State::Unsupported),
        body: Body::cannot_show(i18n, State::Unsupported, false, false, Vec::new())
            .expect("no preview kind"),
    }
}

pub type Host<'a> = (
    &'a mut VisualTestContext,
    Rc<RefCell<Seen>>,
    Entity<QuickLook>,
    Rc<RefCell<Vec<Event>>>,
);

/// A 1100 by 700 window: the listing under Quick Look open on subject.
pub fn host(cx: &mut TestAppContext, subject: Subject, arm: Arm) -> Host<'_> {
    let seen = Rc::new(RefCell::new(Seen::default()));
    let events = Rc::new(RefCell::new(Vec::new()));
    let (held, heard) = (seen.clone(), events.clone());
    let (root, cx) = cx.add_window_view(move |_, cx| {
        let look = cx.new(|_| QuickLook::new(Arc::new(crate::test_catalogs::english())));
        cx.subscribe(&look, move |_, _, event: &Event, _| {
            heard.borrow_mut().push(*event)
        })
        .detach();
        Root {
            seen: held,
            look,
            arm,
        }
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    // The pointer starts off the window, so a hover the listing reports later is one the overlay let through.
    cx.simulate_mouse_move(point(px(-1.), px(-1.)), None, Modifiers::none());
    let look = root.read_with(cx, |root, _| root.look.clone());
    look.update(cx, |look, cx| look.open(subject, true, cx));
    cx.run_until_parked();
    // Geometry and pointer fixtures begin after the opening reveal has settled.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(180));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    *seen.borrow_mut() = Seen::default();
    (cx, seen, look, events)
}

/// The issue's gestures at one point: a move, a click, a double click, a right click and a wheel.
pub fn gestures(cx: &mut VisualTestContext, at: Point<Pixels>) {
    let none = Modifiers::none();
    cx.simulate_mouse_move(at, None, none);
    for clicks in [1, 2] {
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: none,
            click_count: clicks,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: none,
            click_count: clicks,
        });
    }
    cx.simulate_mouse_down(at, MouseButton::Right, none);
    cx.simulate_mouse_up(at, MouseButton::Right, none);
    let wheel = ScrollDelta::Pixels(point(px(0.), px(-120.)));
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: wheel,
        modifiers: none,
        touch_phase: TouchPhase::Moved,
    });
    cx.simulate_event(MouseMoveEvent {
        position: at + point(px(4.), px(4.)),
        pressed_button: None,
        modifiers: none,
    });
    cx.run_until_parked();
}
