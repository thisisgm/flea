//! A native Bottom-layer destination identifies desktop tab drops without guessing from cancellation.
use super::transfer;
use crate::state::Shell;
use gpui::{
    div,
    layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
    prelude::*,
    px, size, AnyWindowHandle, App, AppContext, Bounds, Context, DisplayId, Pixels, Point, Render,
    Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
};
use std::{cell::RefCell, rc::Rc};

// Each catcher with its display's compositor origin, which turns a catcher-local point into a global one.
type Mapped = Vec<(AnyWindowHandle, Point<Pixels>)>;

#[derive(Clone, Default)]
pub(crate) struct Catchers {
    windows: Rc<RefCell<Mapped>>,
    payload: Rc<RefCell<Option<String>>>,
}

impl Catchers {
    /// The live Bottom-layer catchers, one per display while a tab drag is out.
    pub fn count(&self) -> usize {
        self.windows.borrow().len()
    }

    pub fn contains(&self, window: AnyWindowHandle) -> bool {
        self.windows
            .borrow()
            .iter()
            .any(|(handle, _)| *handle == window)
    }

    /// The drop's global point: the catcher's display origin plus the position inside the catcher.
    pub fn global(&self, window: AnyWindowHandle, at: Point<Pixels>) -> Option<Point<Pixels>> {
        let windows = self.windows.borrow();
        let (_, origin) = windows.iter().find(|(handle, _)| *handle == window)?;
        Some(*origin + at)
    }

    // The shared drag router names windows by id.
    pub fn handle(&self, window: u64) -> Option<AnyWindowHandle> {
        self.windows
            .borrow()
            .iter()
            .map(|(handle, _)| *handle)
            .find(|handle| handle.window_id().as_u64() == window)
    }

    pub fn accepts(&self, window: AnyWindowHandle, payload: &str) -> bool {
        self.contains(window) && self.payload.borrow().as_deref() == Some(payload)
    }

    pub fn open(&self, payload: &str, cx: &mut App) {
        self.close(cx);
        for display in cx.displays() {
            if let Ok(window) = cx.open_window(options(display.id()), |_, cx| cx.new(|_| Catcher)) {
                self.windows
                    .borrow_mut()
                    .push((window.into(), display.bounds().origin));
            }
        }
        if !self.windows.borrow().is_empty() {
            *self.payload.borrow_mut() = Some(payload.into());
        }
    }

    pub fn close(&self, cx: &mut App) {
        *self.payload.borrow_mut() = None;
        let windows = std::mem::take(&mut *self.windows.borrow_mut());
        for (window, _) in windows {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
    }
}

pub(crate) fn options(display: DisplayId) -> WindowOptions {
    WindowOptions {
        app_id: Some(crate::window::APP_ID.into()),
        display_id: Some(display),
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            Point::default(),
            size(px(0.), px(0.)),
        ))),
        focus: false,
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "flea-tab-tearoff".into(),
            layer: Layer::Bottom,
            anchor: Anchor::all(),
            exclusive_zone: Some(px(-1.)),
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        ..Default::default()
    }
}

struct Catcher;
impl Render for Catcher {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

impl Shell {
    /// A catcher drop asks the committed policy where it landed against the source window; only a tear-off outcome spawns.
    pub(crate) fn take_desktop_tab(
        &mut self,
        payload: &str,
        at: Point<Pixels>,
    ) -> Option<transfer::TearOff> {
        if payload != self.strip.bar.out_payload {
            return None;
        }
        let (rect, strip) = (self.strip.geometry.rect, self.strip.geometry.strip);
        let now = self.tab_now();
        self.with_tabs(|bar, panes| {
            bar.catcher_drop(panes, rect, strip, f64::from(at.x), f64::from(at.y), now)
        })
    }
}
