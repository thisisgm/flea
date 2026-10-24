use super::bench::{held, open, real_cx};
use std::rc::Rc;

struct InheritedProbe {
    list: Rc<std::cell::RefCell<super::ListState>>,
}

impl gpui::Render for InheritedProbe {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{ParentElement, Styled};
        let theme = crate::palette::Theme::default();
        gpui::div()
            .size_full()
            .text_color(theme.background)
            .text_bg(theme.foreground)
            .child(super::ListElement::new(
                held(Vec::new()),
                0.,
                0,
                Rc::clone(&self.list),
            ))
    }
}

fn painted_background(
    cx: &mut gpui::TestAppContext,
    window: gpui::AnyWindowHandle,
) -> Option<gpui::Hsla> {
    use gpui::AppContext;
    cx.update_window(window, |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let quads = window.painted_quads();
        assert_eq!(
            quads.len(),
            1,
            "an empty list lost its only background quad"
        );
        quads[0].background.as_solid()
    })
    .expect("the probe window is open")
}

#[test]
fn default_root_paints_a_contrasting_canvas() {
    let mut cx = real_cx();
    let window = open(&mut cx, held(Vec::new()), 320., 120.);
    let theme = crate::palette::Theme::default();
    assert_eq!(
        painted_background(&mut cx, window.into()),
        Some(theme.background)
    );
    let palette = super::Palette::inherited(&gpui::TextStyle::default(), theme);
    assert_eq!(
        (palette.background, palette.foreground),
        (theme.background, theme.foreground)
    );
    assert_ne!(theme.background, theme.foreground);
}

#[test]
fn root_uses_the_installed_theme() {
    let mut cx = real_cx();
    let original = crate::palette::Theme::default();
    let theme = crate::palette::Theme {
        background: original.foreground,
        foreground: original.background,
        ..original
    };
    cx.update(|cx| cx.set_global(theme));
    let window = open(&mut cx, held(Vec::new()), 320., 120.);
    assert_eq!(
        painted_background(&mut cx, window.into()),
        Some(theme.background)
    );
    let palette = super::Palette::inherited(&gpui::TextStyle::default(), theme);
    assert_eq!(
        (palette.background, palette.foreground),
        (theme.background, theme.foreground)
    );
}

#[test]
fn inherited_background_keeps_its_text_colour() {
    let mut cx = real_cx();
    let theme = crate::palette::Theme::default();
    let list = Rc::new(std::cell::RefCell::new(super::ListState::new(
        super::bench::bench_typeface(),
    )));
    let window = cx.open_window(gpui::size(gpui::px(320.), gpui::px(120.)), |_, _| {
        InheritedProbe { list }
    });
    assert_eq!(
        painted_background(&mut cx, window.into()),
        Some(theme.foreground)
    );
    let style = gpui::TextStyle {
        color: theme.background,
        background_color: Some(theme.foreground),
        ..Default::default()
    };
    let palette = super::Palette::inherited(&style, crate::palette::Theme::default());
    assert_eq!(
        (palette.background, palette.foreground),
        (theme.foreground, theme.background)
    );
}
