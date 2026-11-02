//! Ruling 145 keeps the About hero's original spiral while listing loading uses Omi.
use super::{tests::panel, view::SettingsView};
use gpui::{Bounds, Hsla, Pixels, TestAppContext};
use std::cell::RefCell;

struct Painted {
    data: &'static [u8],
    bounds: Bounds<Pixels>,
    ink: Hsla,
}

thread_local! {
    static CAPTURE: RefCell<Option<Vec<Painted>>> = const { RefCell::new(None) };
}

pub(super) fn record(key: &str, data: &'static [u8], bounds: Bounds<Pixels>, ink: Hsla) {
    if key == "flea-st1:flea" {
        CAPTURE.with_borrow_mut(|capture| {
            if let Some(capture) = capture {
                capture.push(Painted { data, bounds, ink });
            }
        });
    }
}

#[gpui::test]
fn sol_gap64_about_keeps_the_original_spiral_asset_and_hero_size(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| {
        let mut panel = panel("{}");
        panel.show_section("about");
        SettingsView::new(panel, cx)
    });
    cx.run_until_parked();
    CAPTURE.with_borrow_mut(|capture| *capture = Some(Vec::new()));
    let accent = cx.update(|window, cx| {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let accent = crate::list::Palette::inherited(&window.text_style(), theme).accent;
        window.refresh();
        window.draw(cx).clear(cx);
        accent
    });
    let painted = CAPTURE.with_borrow_mut(|capture| capture.take().unwrap());
    let hero = painted
        .iter()
        .find(|painted| f32::from(painted.bounds.size.width) == 38.)
        .expect("the real About frame paints its original 38 px hero");
    let expected = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M21 21H3V3h18v14H7V7h10v6h-6"/></svg>"##;
    assert_eq!(hero.data, expected);
    assert_eq!(f32::from(hero.bounds.size.height), 38.);
    assert_eq!(hero.ink, accent);
    view.read_with(cx, |view, _| assert_eq!(view.panel.section, "about"));
}
