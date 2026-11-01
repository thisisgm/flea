//! A child of the actual card shell observes its inherited GPUI text style during layout.
use super::{
    palette::CardPalette,
    view::{self, Pointer, Scene},
};
use crate::{list::Typeface, palette::Theme};
use flea::ui_logic_op1::card::Kind;
use gpui::{
    prelude::*, px, size, App, AppContext, Bounds, Context, Element, ElementId, GlobalElementId,
    InspectorElementId, LayoutId, Pixels, Render, Style, TestAppContext, TextStyle, Window,
};
use std::{cell::RefCell, rc::Rc};

struct Probe(Rc<RefCell<Option<TextStyle>>>);

impl IntoElement for Probe {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Probe {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        *self.0.borrow_mut() = Some(window.text_style());
        (window.request_layout(Style::default(), None, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

struct Card(Rc<RefCell<Option<TextStyle>>>);

impl Render for Card {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let typeface = Typeface::for_window(window, cx);
        let palette = CardPalette::from_theme(&window.text_style(), Theme::default(), 8.);
        let scene = Scene {
            copy_field: None,
            frame: None,
            typeface: &typeface,
            palette: &palette,
            catalog: &crate::test_catalogs::english(),
            pointer: &Pointer::default(),
            wire: &|_, button| button,
            pinned: None,
            feedback: None,
        };
        view::shell(
            &scene,
            Kind::CopyTo,
            (1100., 700.),
            "Copy To".into(),
            vec![Probe(self.0.clone()).into_any_element()],
            Vec::new(),
        )
    }
}

#[gpui::test]
fn fix9_card_inherits_the_oem_family_and_live_body(cx: &mut TestAppContext) {
    let seen = Rc::new(RefCell::new(None));
    let view = cx.add_window(|window, _| {
        window.set_rem_size(px(14.));
        Card(seen.clone())
    });
    cx.simulate_window_resize(view.into(), size(px(1100.), px(700.)));
    for base in [12., 14., 16.] {
        view.update(cx, |_, window, cx| {
            window.set_rem_size(px(base));
            cx.notify();
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(view.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        let style = seen.borrow().clone().expect("the body laid out");
        assert_eq!(style.font_family, Typeface::omarchy_default().font.family);
        assert_eq!(style.font_size.to_pixels(px(base)), px(base));
    }
}

struct Caption(Rc<RefCell<Option<TextStyle>>>);

impl Render for Caption {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let typeface = Typeface::for_window(window, cx);
        let palette = CardPalette::from_theme(&window.text_style(), Theme::default(), 8.);
        let scene = Scene {
            copy_field: None,
            frame: None,
            typeface: &typeface,
            palette: &palette,
            catalog: &crate::test_catalogs::english(),
            pointer: &Pointer::default(),
            wire: &|_, button| button,
            pinned: None,
            feedback: None,
        };
        let line = view::caption(&scene, "Explain".into(), palette.foreground)
            .child(Probe(self.0.clone()).into_any_element());
        view::shell(
            &scene,
            Kind::Collide,
            (1100., 700.),
            "Keep both".into(),
            vec![line.into_any_element()],
            Vec::new(),
        )
    }
}

#[gpui::test]
fn ey254_the_collision_body_line_grows_with_the_text_size(cx: &mut TestAppContext) {
    let seen = Rc::new(RefCell::new(None));
    let view = cx.add_window(|window, _| {
        window.set_rem_size(px(14.));
        Caption(seen.clone())
    });
    cx.simulate_window_resize(view.into(), size(px(1100.), px(700.)));
    // The title scales with the text size; the body line takes the caption token, 12 at 14 and 17 at 20.
    for (base, drawn) in [(14., 12.), (20., 17.)] {
        view.update(cx, |_, window, cx| {
            window.set_rem_size(px(base));
            cx.notify();
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(view.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        let style = seen.borrow().clone().expect("the body line laid out");
        assert_eq!(
            style.font_size.to_pixels(px(base)),
            px(drawn),
            "the collision body line at text size {base}"
        );
    }
}

struct Sentence(Rc<RefCell<Option<TextStyle>>>);

impl Render for Sentence {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let typeface = Typeface::for_window(window, cx);
        let palette = CardPalette::from_theme(&window.text_style(), Theme::default(), 8.);
        let scene = Scene {
            copy_field: None,
            frame: None,
            typeface: &typeface,
            palette: &palette,
            catalog: &crate::test_catalogs::english(),
            pointer: &Pointer::default(),
            wire: &|_, button| button,
            pinned: None,
            feedback: None,
        };
        let line = view::sentence(
            "1 item, 99 B. This deletes it from disk. This cannot be undone.".into(),
        )
        .child(Probe(self.0.clone()).into_any_element());
        view::shell(
            &scene,
            Kind::Delete,
            (1100., 700.),
            "Empty Trash?".into(),
            vec![line.into_any_element()],
            Vec::new(),
        )
    }
}

// EY-350: the Delete card's sentence is drawn at the buttons' live body size at every text size, never a fixed 14.
#[gpui::test]
fn ey350_the_delete_sentence_takes_the_live_body_size(cx: &mut TestAppContext) {
    let seen = Rc::new(RefCell::new(None));
    let view = cx.add_window(|window, _| {
        window.set_rem_size(px(14.));
        Sentence(seen.clone())
    });
    cx.simulate_window_resize(view.into(), size(px(1100.), px(700.)));
    for base in [12., 14., 20.] {
        view.update(cx, |_, window, cx| {
            window.set_rem_size(px(base));
            cx.notify();
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(view.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        let style = seen.borrow().clone().expect("the sentence laid out");
        assert_eq!(
            style.font_size.to_pixels(px(base)),
            px(base),
            "the Delete sentence at text size {base}"
        );
        let line = style.line_height_in_pixels(px(base));
        // GPUI snaps the line to whole pixels, so the 20/14 ratio holds within one.
        assert!(
            (f32::from(line) - base * 20. / 14.).abs() < 1.,
            "its line at text size {base}: {line:?}"
        );
    }
}
