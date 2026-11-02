//! An Open With icon must reach the image atlas through the actual menu paint path.
use super::{
    element::MenuElement,
    lines::Env,
    menu::{Face, Menu},
};
use crate::list::Typeface;
use flea::ui_logic_mn1::{fit::Rect, Entry, Text};
use gpui::{
    div, prelude::*, px, Context, ImageAssetLoader, Render, Resource, TestAppContext, Window,
};
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::Arc};

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
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/vf-more-app-icon.png");
    let source = Resource::Path(Arc::from(path.as_path()));
    let menu = Rc::new(RefCell::new(Menu::default()));
    let shared = menu.clone();
    let handle = cx.add_window(move |_, _| Host(shared));
    cx.update_window(handle.into(), |_, _, cx| {
        let _ = cx.fetch_asset::<ImageAssetLoader>(&source);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
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
                icon: Some(path.to_string_lossy().into_owned()),
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
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    // The first menu paint starts the asynchronous icon resolver; the next paints its answer.
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        let image = cx
            .fetch_asset::<ImageAssetLoader>(&source)
            .unwrap()
            .unwrap();
        window.refresh();
        window.draw(cx).clear(cx);
        assert!(
            window.has_image_atlas_entry(&image),
            "the app icon was replaced by the generic window glyph"
        );
    })
    .unwrap();
}
