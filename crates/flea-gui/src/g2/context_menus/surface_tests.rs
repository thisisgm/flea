//! Menu ink uses the HIG surface role, while retaining the specific error role.
use super::element::frames;
use crate::palette::Theme;
use gpui::{div, Context, IntoElement, Render, TestAppContext, Window};

struct Probe;
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[gpui::test]
fn openwith_parent_and_flyout_use_the_theme_surface(cx: &mut TestAppContext) {
    let theme = Theme {
        background: Theme::from_text("background='#172820'").background,
        surface: Theme::from_text("background='#071008'").background,
        ..Theme::default()
    };
    cx.update(|cx| cx.set_global(theme));
    let window = cx.add_window(|_, _| Probe);
    window
        .update(cx, |_, window, cx| {
            // The card, its flyout and a retired flyout all paint with this one ink.
            let ink = frames::ink(window, cx);
            assert_eq!(
                ink.surface, theme.surface,
                "parent and flyout keep the surface role"
            );
            assert_eq!(
                ink.error, theme.error,
                "the named unavailable application's red role is preserved"
            );
        })
        .unwrap();
}
