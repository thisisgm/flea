//! The shared test parts: theme snapshots, a recording backend, and the frame helpers every view test uses.
use flea::i18n::I18n;
use flea_gui::g2::doc_preview::code::{self, Mark};
use gpui::{px, App, AppContext as _, Render, SharedString, TestAppContext, WindowHandle};
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub const SVG: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16'><rect width='16' height='16'/></svg>";

pub fn mark(name: &str) -> Mark {
    (SharedString::from(name.to_owned()), SVG)
}

pub fn catalog() -> Arc<I18n> {
    Arc::new(crate::test_catalogs::english())
}

pub fn typeface() -> code::Typeface {
    code::Typeface {
        font: gpui::font("monospace"),
        body: px(14.),
        caption: px(12.),
    }
}

pub fn palette() -> code::Palette {
    let ink = flea_gui::palette::Theme::default().foreground;
    code::Palette {
        foreground: ink,
        muted: ink.opacity(0.6),
        muted_frame: ink.opacity(0.3),
        gutter: ink.opacity(0.7),
        inks: [ink; 7],
    }
}

/// Board ql-code: 14 inset, 41 top under the bar, 20 right, rows 20.
pub const METRICS: code::Metrics = code::Metrics {
    inset: 14.,
    top: 41.,
    right: 20.,
    row: 20.,
    gap: 9.,
    state_mark: 32.,
};

/// Every call the views made, in order.
pub type Calls<T> = Rc<RefCell<Vec<T>>>;

pub fn draw<V: Render>(cx: &mut TestAppContext, view: WindowHandle<V>) {
    cx.update_window(view.into(), |_, window, _| window.refresh())
        .unwrap();
    cx.run_until_parked();
}

pub fn with<V: Render, R>(
    cx: &mut TestAppContext,
    view: WindowHandle<V>,
    f: impl FnOnce(&V) -> R,
) -> R {
    view.update(cx, |view: &mut V, _, _| f(view)).unwrap()
}

/// The test window's device pixel ratio.
pub fn scale<V: Render>(cx: &mut TestAppContext, view: WindowHandle<V>) -> f32 {
    cx.update_window(view.into(), |_, window, _| window.scale_factor())
        .unwrap()
}

/// Reads a view that answers from its child entities, with the app to read them through.
pub fn look<V: Render, R>(
    cx: &mut TestAppContext,
    view: WindowHandle<V>,
    f: impl FnOnce(&V, &App) -> R,
) -> R {
    view.update(cx, |view: &mut V, _, cx| f(view, cx)).unwrap()
}
