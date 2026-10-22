//! A child of the actual card shell observes its inherited GPUI text style during layout.
use super::{
    palette::CardPalette,
    view::{self, Pointer, Scene},
};
use crate::{list::Typeface, palette::Theme};
use flea::{i18n::I18n, ui_logic_op1::card::Kind};
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
            typeface: &typeface,
            palette: &palette,
            catalog: &I18n::default(),
            pointer: &Pointer::default(),
            wire: &|_, button| button,
            pinned: None,
            since: cx.background_executor().now(),
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
