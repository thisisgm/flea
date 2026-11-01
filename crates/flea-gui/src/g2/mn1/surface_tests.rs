//! Menu prepaint uses the HIG surface role, while retaining the specific error role.
use super::{element::MenuElement, menu::Menu};
use crate::palette::Theme;
use gpui::{div, px, Bounds, Context, Element, IntoElement, Render, TestAppContext, Window};
use std::{cell::RefCell, rc::Rc};

struct Probe;
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[gpui::test]
fn openwith_parent_and_flyout_use_the_theme_surface(cx: &mut TestAppContext) {
    let theme = Theme {
        background: gpui::rgb(0x172820).into(),
        surface: gpui::rgb(0x071008).into(),
        ..Theme::default()
    };
    cx.update(|cx| cx.set_global(theme));
    let window = cx.add_window(|_, _| Probe);
    window
        .update(cx, |_, window, cx| {
            let menu = Rc::new(RefCell::new(Menu::default()));
            for mut element in [
                MenuElement::new(menu.clone(), px(0.)),
                MenuElement::fading(menu, px(0.)),
            ] {
                let ink = element.prepaint(None, None, Bounds::default(), &mut (), window, cx);
                assert_eq!(
                    ink.surface, theme.surface,
                    "parent and flyout keep the surface role"
                );
                assert_eq!(
                    ink.error, theme.error,
                    "the named unavailable application's red role is preserved"
                );
            }
        })
        .unwrap();
}
