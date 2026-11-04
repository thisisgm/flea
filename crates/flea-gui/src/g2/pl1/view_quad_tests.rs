//! The unlock buttons' painted quads from the real UnlockView in a test window: rest, hover, press, press held off, focus.
use super::ink::CardInk;
use super::process::{Handle, Job, Runner};
use super::unlock::{Part, Phase};
use super::view::UnlockView;
use crate::list::Palette;
use flea::i18n::{I18n, Paths};
use gpui::{
    point, px, Bounds, Entity, Hsla, Modifiers, MouseButton, Point, Quad, ScaledPixels,
    TestAppContext, VisualTestContext,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

// v0.3.12 ui/js/Buttons.js:14-16, written out so a changed constant in card.rs cannot move the pin with it.
const ORACLE_HOVER: f32 = 0.08;
const ORACLE_PRESS: f32 = 0.14;
// Buttons.js RING: a focused primary whose rest frame is the foreground draws its frame this wide.
const ORACLE_RING: f32 = 2.;

struct Idle;

/// Records each job's argv and answers nothing, so a click's child start is visible to the test.
#[derive(Default)]
struct Starts(RefCell<Vec<Vec<String>>>);

impl Runner for Starts {
    fn start(&self, job: Job) -> Handle {
        self.0.borrow_mut().push(job.argv);
        Handle::detached().0
    }
}

impl Runner for Idle {
    fn start(&self, _: Job) -> Handle {
        Handle::detached().0
    }
}

pub(super) struct Drawn {
    ink: CardInk,
    pub(super) scale: f32,
}

impl Drawn {
    fn hover(&self) -> Hsla {
        self.ink.foreground.opacity(ORACLE_HOVER)
    }

    fn press(&self) -> Hsla {
        self.ink.foreground.opacity(ORACLE_PRESS)
    }

    fn at(&self, p: Point<ScaledPixels>) -> Point<gpui::Pixels> {
        point(px(p.x.0 / self.scale), px(p.y.0 / self.scale))
    }
}

fn same(a: Hsla, b: Hsla) -> bool {
    (a.h - b.h).abs() < 1e-3
        && (a.s - b.s).abs() < 1e-3
        && (a.l - b.l).abs() < 1e-3
        && (a.a - b.a).abs() < 1e-3
}

pub(super) fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

fn filled(quads: &[Quad], colour: Hsla) -> Vec<Bounds<ScaledPixels>> {
    quads
        .iter()
        .filter(|q| q.background.as_solid().is_some_and(|b| same(b, colour)))
        .map(|q| q.bounds)
        .collect()
}

fn inside(quads: &[Quad], colour: Hsla, frame: Bounds<ScaledPixels>) -> Vec<Bounds<ScaledPixels>> {
    filled(quads, colour)
        .into_iter()
        .filter(|b| b.is_contained_within(&frame) && *b != frame)
        .collect()
}

pub(super) fn paint(cx: &mut VisualTestContext) -> Vec<Quad> {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.painted_quads()
    })
}

/// An open card with one typed character, so Unlock is available, and an optional focused part.
pub(super) fn card(
    cx: &mut TestAppContext,
    focus: Option<Part>,
) -> (Entity<UnlockView>, &mut VisualTestContext, Drawn) {
    opened(cx, "x", focus)
}

/// An open card with `typed` in its field and an optional focused part.
fn opened<'a>(
    cx: &'a mut TestAppContext,
    typed: &str,
    focus: Option<Part>,
) -> (Entity<UnlockView>, &'a mut VisualTestContext, Drawn) {
    let catalog = Arc::new(I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent/omarchy"),
        language_file: PathBuf::from("/nonexistent/language"),
        catalogs: Vec::new(),
    }));
    let runner: Rc<dyn Runner> = Rc::new(Idle);
    let (view, cx) = cx.add_window_view(|_, cx| UnlockView::new(catalog, runner, cx));
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_for("/dev/sdb1", "sdb1", 0, "crypto_LUKS", window, cx);
            let card = v.host.card.as_mut().unwrap();
            card.type_text(typed);
            if focus.is_some() {
                card.focus = focus;
            }
            cx.notify();
        })
    });
    let drawn = cx.update(|window, cx| {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        Drawn {
            ink: CardInk::inherited(&Palette::inherited(&window.text_style(), theme)),
            scale: window.scale_factor(),
        }
    });
    (view, cx, drawn)
}

/// The widest border any quad paints over these bounds; GPUI paints each frame side as its own quad.
fn border(quads: &[Quad], b: Bounds<ScaledPixels>) -> f32 {
    quads
        .iter()
        .filter(|q| q.bounds == b)
        .flat_map(|q| {
            [
                q.border_widths.top.0,
                q.border_widths.right.0,
                q.border_widths.bottom.0,
                q.border_widths.left.0,
            ]
        })
        .fold(0., f32::max)
}

/// The Unlock frame (the framed bounds on the accent wash) and the framed Cancel left of it on the same line.
pub(super) fn frames(quads: &[Quad], d: &Drawn) -> (Bounds<ScaledPixels>, Bounds<ScaledPixels>) {
    let mut framed: Vec<Bounds<ScaledPixels>> = Vec::new();
    for q in quads
        .iter()
        .filter(|q| q.border_widths.top.0 > 0. || q.border_widths.left.0 > 0.)
    {
        if !framed.contains(&q.bounds) {
            framed.push(q.bounds);
        }
    }
    let unlock: Vec<_> = framed
        .iter()
        .copied()
        .filter(|b| filled(quads, d.ink.wash).contains(b))
        .collect();
    assert_eq!(
        unlock.len(),
        1,
        "the available Unlock rests on one framed accent wash"
    );
    let unlock = unlock[0];
    let cancel: Vec<_> = framed
        .iter()
        .copied()
        .filter(|b| {
            b.origin.y == unlock.origin.y
                && b.size.height == unlock.size.height
                && b.origin.x < unlock.origin.x
        })
        .collect();
    assert_eq!(cancel.len(), 1, "one framed Cancel left of Unlock");
    (unlock, cancel[0])
}

#[gpui::test]
fn at_rest_neither_button_draws_a_pointer_wash(cx: &mut TestAppContext) {
    let (_view, cx, d) = card(cx, None);
    let quads = paint(cx);
    let (unlock, cancel) = frames(&quads, &d);
    for frame in [unlock, cancel] {
        assert!(
            inside(&quads, d.hover(), frame).is_empty()
                && inside(&quads, d.press(), frame).is_empty()
        );
    }
}

#[gpui::test]
fn a_hovered_unlock_lays_the_8_percent_wash_inside_its_kept_accent_wash(cx: &mut TestAppContext) {
    let (_view, cx, d) = card(cx, None);
    let (unlock, _) = frames(&paint(cx), &d);
    cx.simulate_mouse_move(d.at(unlock.center()), None, Modifiers::default());
    let quads = paint(cx);
    assert_eq!(
        frames(&quads, &d).0,
        unlock,
        "the hovered Unlock keeps its accent wash on the frame"
    );
    assert_eq!(
        inside(&quads, d.hover(), unlock).len(),
        1,
        "the hover wash lies inside the frame, over the accent wash"
    );
    assert_eq!(filled(&quads, d.hover()).len(), 1, "and nowhere else");
}

#[gpui::test]
fn a_pressed_unlock_lays_the_14_percent_wash_in_place_of_the_hover_wash(cx: &mut TestAppContext) {
    let (_view, cx, d) = card(cx, None);
    let (unlock, _) = frames(&paint(cx), &d);
    cx.simulate_mouse_move(d.at(unlock.center()), None, Modifiers::default());
    cx.simulate_mouse_down(
        d.at(unlock.center()),
        MouseButton::Left,
        Modifiers::default(),
    );
    let quads = paint(cx);
    assert_eq!(
        frames(&quads, &d).0,
        unlock,
        "the pressed Unlock keeps its accent wash on the frame"
    );
    assert_eq!(
        inside(&quads, d.press(), unlock).len(),
        1,
        "the press wash lies inside the frame"
    );
    assert!(
        inside(&quads, d.hover(), unlock).is_empty(),
        "the press wash replaces the hover wash, never stacks on it"
    );
}

#[gpui::test]
fn a_press_held_outside_the_button_draws_the_resting_button(cx: &mut TestAppContext) {
    let (_view, cx, d) = card(cx, None);
    let (unlock, _) = frames(&paint(cx), &d);
    cx.simulate_mouse_move(d.at(unlock.center()), None, Modifiers::default());
    cx.simulate_mouse_down(
        d.at(unlock.center()),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        point(px(1.), px(1.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    let quads = paint(cx);
    assert_eq!(frames(&quads, &d).0, unlock);
    assert!(
        inside(&quads, d.press(), unlock).is_empty()
            && inside(&quads, d.hover(), unlock).is_empty(),
        "held outside, as DialogButton pressShown"
    );
    cx.simulate_mouse_move(
        d.at(unlock.center()),
        MouseButton::Left,
        Modifiers::default(),
    );
    assert_eq!(
        inside(&paint(cx), d.press(), unlock).len(),
        1,
        "back inside while held, the press wash returns"
    );
}

#[gpui::test]
fn cancel_takes_the_same_pointer_washes_with_no_resting_wash(cx: &mut TestAppContext) {
    let (_view, cx, d) = card(cx, None);
    let (_, cancel) = frames(&paint(cx), &d);
    cx.simulate_mouse_move(d.at(cancel.center()), None, Modifiers::default());
    let quads = paint(cx);
    assert_eq!(
        inside(&quads, d.hover(), cancel).len(),
        1,
        "Cancel hover wash inside its frame"
    );
    assert!(
        quads
            .iter()
            .filter(|q| q.bounds == cancel)
            .all(|q| q.background.as_solid().is_none_or(|b| b.a < 1e-6)),
        "Cancel has no resting wash"
    );
    cx.simulate_mouse_down(
        d.at(cancel.center()),
        MouseButton::Left,
        Modifiers::default(),
    );
    let quads = paint(cx);
    assert_eq!(
        inside(&quads, d.press(), cancel).len(),
        1,
        "Cancel press wash inside its frame"
    );
    assert!(inside(&quads, d.hover(), cancel).is_empty());
}

#[gpui::test]
fn a_focused_unlock_whose_rest_frame_is_the_foreground_draws_the_ring(cx: &mut TestAppContext) {
    let (_view, cx, d) = card(cx, Some(Part::Unlock));
    assert!(
        same(d.ink.accent_frame, d.ink.foreground),
        "the interim palette draws the accent frame in the foreground"
    );
    let quads = paint(cx);
    let (unlock, cancel) = frames(&quads, &d);
    let width = |b: Bounds<ScaledPixels>| border(&quads, b);
    assert!(
        close(width(unlock), ORACLE_RING * d.scale),
        "focus would otherwise match the resting hairline"
    );
    assert!(
        close(width(cancel), d.scale),
        "an unfocused Cancel keeps the hairline"
    );
    cx.simulate_mouse_move(d.at(unlock.center()), None, Modifiers::default());
    let wash = inside(&paint(cx), d.hover(), unlock);
    assert_eq!(wash.len(), 1);
    assert!(
        close(
            wash[0].origin.x.0 - unlock.origin.x.0,
            ORACLE_RING * d.scale
        ),
        "the pointer wash is inset by the ring"
    );
}

#[gpui::test]
fn a_focused_cancel_draws_a_foreground_hairline(cx: &mut TestAppContext) {
    let (_view, cx, d) = card(cx, Some(Part::Cancel));
    let quads = paint(cx);
    let (_, cancel) = frames(&quads, &d);
    assert!(
        close(border(&quads, cancel), d.scale),
        "a secondary's rest frame is not the foreground, so focus stays one hairline"
    );
    assert!(quads
        .iter()
        .filter(|q| q.bounds == cancel && q.border_widths.top.0 > 0.)
        .all(|q| same(q.border_color, d.ink.foreground)));
}

#[gpui::test]
fn an_unavailable_unlock_answers_no_pointer(cx: &mut TestAppContext) {
    let (_view, cx, d) = opened(cx, "", None);
    let quads = paint(cx);
    assert!(
        filled(&quads, d.ink.wash).is_empty(),
        "an unavailable Unlock rests with no accent wash"
    );
    let mut row: Vec<Bounds<ScaledPixels>> = Vec::new();
    for q in quads.iter().filter(|q| q.border_widths.top.0 > 0.) {
        let paired = quads.iter().any(|o| {
            o.bounds != q.bounds
                && o.border_widths.top.0 > 0.
                && o.bounds.origin.y == q.bounds.origin.y
                && o.bounds.size.height == q.bounds.size.height
        });
        if paired && !row.contains(&q.bounds) {
            row.push(q.bounds);
        }
    }
    assert_eq!(row.len(), 2, "Cancel and Unlock share the buttons row");
    let unlock = if row[0].origin.x > row[1].origin.x {
        row[0]
    } else {
        row[1]
    };
    cx.simulate_mouse_move(d.at(unlock.center()), None, Modifiers::default());
    assert!(
        inside(&paint(cx), d.hover(), unlock).is_empty(),
        "no hover wash"
    );
    cx.simulate_mouse_down(
        d.at(unlock.center()),
        MouseButton::Left,
        Modifiers::default(),
    );
    let quads = paint(cx);
    assert!(
        inside(&quads, d.press(), unlock).is_empty()
            && inside(&quads, d.hover(), unlock).is_empty(),
        "no press wash"
    );
}

/// The framed buttons on the row, Cancel left of Unlock, whichever state they draw in.
fn buttons(quads: &[Quad]) -> (Bounds<ScaledPixels>, Bounds<ScaledPixels>) {
    let mut framed: Vec<Bounds<ScaledPixels>> = Vec::new();
    for q in quads.iter().filter(|q| q.border_widths.top.0 > 0.) {
        if !framed.contains(&q.bounds) {
            framed.push(q.bounds);
        }
    }
    let row: Vec<_> = framed
        .iter()
        .copied()
        .filter(|b| {
            framed
                .iter()
                .any(|o| o != b && o.origin.y == b.origin.y && o.size.height == b.size.height)
        })
        .collect();
    assert_eq!(row.len(), 2, "Cancel and Unlock share one framed row");
    if row[0].origin.x < row[1].origin.x {
        (row[0], row[1])
    } else {
        (row[1], row[0])
    }
}

/// An open card on a recording runner, with `typed` in its field.
fn clickable<'a>(
    cx: &'a mut TestAppContext,
    typed: &str,
) -> (
    Rc<Starts>,
    Entity<UnlockView>,
    &'a mut VisualTestContext,
    Drawn,
) {
    let catalog = Arc::new(I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent/omarchy"),
        language_file: PathBuf::from("/nonexistent/language"),
        catalogs: Vec::new(),
    }));
    let starts = Rc::new(Starts::default());
    let runner: Rc<dyn Runner> = starts.clone();
    let (view, cx) = cx.add_window_view(|_, cx| UnlockView::new(catalog, runner, cx));
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_for("/dev/sdb1", "sdb1", 0, "crypto_LUKS", window, cx);
            v.host.card.as_mut().unwrap().type_text(typed);
            cx.notify();
        })
    });
    let drawn = cx.update(|window, cx| {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        Drawn {
            ink: CardInk::inherited(&Palette::inherited(&window.text_style(), theme)),
            scale: window.scale_factor(),
        }
    });
    (starts, view, cx, drawn)
}

#[gpui::test]
fn a_click_on_unlock_starts_the_unlock_child(cx: &mut TestAppContext) {
    let (starts, view, cx, d) = clickable(cx, "x");
    let (_, unlock) = buttons(&paint(cx));
    cx.simulate_click(d.at(unlock.center()), Modifiers::default());
    cx.run_until_parked();
    assert_eq!(
        starts.0.borrow().first().map(|a| a[..2].join(" ")),
        Some("udisksctl unlock".to_string()),
        "the click starts the unlock child"
    );
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).host.card.as_ref().unwrap().phase,
            Phase::Unlocking
        )
    });
}

#[gpui::test]
fn a_click_on_cancel_closes_the_card(cx: &mut TestAppContext) {
    let (_starts, view, cx, d) = clickable(cx, "x");
    let (cancel, _) = buttons(&paint(cx));
    cx.simulate_click(d.at(cancel.center()), Modifiers::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!view.read(cx).host.opened(), "the click closes the card"));
}

#[gpui::test]
fn a_click_on_an_unavailable_unlock_starts_nothing_and_shows_no_refusal(cx: &mut TestAppContext) {
    let (starts, view, cx, d) = clickable(cx, "");
    let (_, unlock) = buttons(&paint(cx));
    cx.simulate_click(d.at(unlock.center()), Modifiers::default());
    cx.run_until_parked();
    assert!(
        starts.0.borrow().is_empty(),
        "an empty field starts no child"
    );
    cx.update(|_, cx| {
        let card = view.read(cx).host.card.as_ref().unwrap();
        assert!(
            card.note.is_none() && !card.field.error,
            "an unavailable Unlock shows no wrong-passphrase note"
        );
        assert_eq!(
            card.phase,
            Phase::Typing,
            "and leaves the card where it was"
        );
    });
}
