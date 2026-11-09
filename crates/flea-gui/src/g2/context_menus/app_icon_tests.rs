//! An Open With icon must reach the image atlas through the actual menu paint path.
use super::{
    app_icons,
    element::MenuElement,
    lines::Env,
    menu::{Face, Menu},
};
use crate::list::Typeface;
use flea::ui_logic_context_menus::{fit::Rect, Entry, Text};
use gpui::{div, prelude::*, px, Context, Render, TestAppContext, Window};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

struct Host(Rc<RefCell<Menu>>);
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(MenuElement::new(self.0.clone(), px(8.)))
    }
}

#[gpui::test]
fn vf_more_open_with_paints_the_desktop_entry_icon(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/vf-more-app-icon.png");
    let icon = path.to_string_lossy().into_owned();
    let menu = Rc::new(RefCell::new(Menu::default()));
    let shared = menu.clone();
    let handle = cx.add_window(move |_, _| Host(shared));
    handle
        .update(cx, |_, window, cx| {
            let catalog = crate::test_catalogs::english();
            let typeface = Typeface::omarchy_default();
            let env = Env {
                catalog: &catalog,
                preset: "default",
                key_hints: false,
                typeface: &typeface,
                inset: 4.,
            };
            menu.borrow_mut().open(
                Face::File,
                vec![Entry {
                    icon: Some(icon.clone()),
                    ..Entry::leaf("nvim.desktop", Text::Data("Neovim".into()))
                }],
                (20., 20.),
                Rect {
                    x: 0.,
                    y: 0.,
                    w: 1100.,
                    h: 700.,
                },
                "a",
                &env,
                window,
            );
            // Only the menu painter is pinned here; openwith_icon_tests drives the flyout request path.
            let slot = menu.borrow().card.as_ref().unwrap().built.metrics.slot;
            app_icons::load([icon.clone()], slot, window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    app_icons::PAINTED.with(|painted| *painted.borrow_mut() = Some(Vec::new()));
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    let painted = app_icons::PAINTED.with(|painted| painted.borrow_mut().take().unwrap());
    assert_eq!(
        painted,
        [icon],
        "the app icon was replaced by the generic window glyph"
    );
}
