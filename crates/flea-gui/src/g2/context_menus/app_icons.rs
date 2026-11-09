//! Open With application icons: each entry's own Icon= file, as the backend's icon index resolved it for v0.3.12, rasterized off the UI thread at the row's mark size.
use crate::{g2::quick_look_overlay, state::Shell};
use flea::ui_logic_context_menus::fit::SLOT;
use flea::ui_logic_open_with_properties::openwith::icons::Icons;
use gpui::{App, Bounds, Context, Global, Pixels, RenderImage, Window};
use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{atomic::AtomicU64, Arc},
};

/// Each icon file at each device size it was asked for: None while its raster is pending or after it failed.
#[derive(Default)]
pub(crate) struct AppIcons {
    held: HashMap<(String, u32), Option<Arc<RenderImage>>>,
    #[cfg(test)]
    completion_gate: Option<async_channel::Receiver<()>>,
}

impl Global for AppIcons {}

#[cfg(test)]
thread_local! {
    pub(crate) static PAINTED: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
    pub(crate) static FALLBACKS: std::cell::RefCell<Option<Vec<(String, gpui::Hsla)>>> = const { std::cell::RefCell::new(None) };
}

/// Tests can hold completion until the initiating window is gone, without delaying or changing production raster work.
#[cfg(test)]
pub(crate) fn hold_completions(cx: &mut App) -> async_channel::Sender<()> {
    let (release, gate) = async_channel::bounded(1);
    cx.default_global::<AppIcons>().completion_gate = Some(gate);
    release
}

/// Whether this icon slot was requested, including a pending or failed raster.
#[cfg(test)]
pub(crate) fn requested(path: &str, window: &Window, cx: &App) -> bool {
    cx.try_global::<AppIcons>()
        .is_some_and(|icons| icons.held.contains_key(&(path.to_owned(), side(window))))
}

/// Only rows intersecting the open flyout's viewport may start icon work, at the flyout's own mark size.
fn visible(menu: &super::menu::Menu) -> (Vec<String>, f32) {
    let Some(flyout) = menu
        .flyout
        .as_ref()
        .filter(|flyout| flyout.action == "openWith")
    else {
        return (Vec::new(), SLOT);
    };
    let card = &flyout.card;
    let metrics = card.built.metrics;
    let mut top = -card.content_y;
    let mut wanted = Vec::new();
    for entry in &card.entries {
        if top >= card.view_h {
            break;
        }
        let height = metrics.height(entry);
        if top + height > 0 {
            if let Some(icon) = entry.icon.as_ref().filter(|icon| !icon.is_empty()) {
                wanted.push(icon.clone());
            }
        }
        top += height;
    }
    (wanted, metrics.slot)
}

/// The mark slot's side in device pixels at the window's scale.
#[cfg(test)]
fn side(window: &Window) -> u32 {
    pixels(SLOT, window)
}

/// The mark slot's raster side includes the card's body scale and the display scale.
fn pixels(logical: f32, window: &Window) -> u32 {
    (logical * window.scale_factor()).round().max(1.) as u32
}

/// The raster this icon file holds at the flyout mark size, once one landed.
#[cfg(test)]
pub(crate) fn held(path: &str, window: &Window, cx: &App) -> Option<Arc<RenderImage>> {
    held_at(path, SLOT, window, cx)
}

fn held_at(path: &str, logical: f32, window: &Window, cx: &App) -> Option<Arc<RenderImage>> {
    cx.try_global::<AppIcons>()?
        .held
        .get(&(path.to_owned(), pixels(logical, window)))?
        .clone()
}

/// Paints the application's icon at its mark size; false leaves its existing window fallback.
pub(super) fn paint(
    path: &str,
    logical: f32,
    at: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let Some(image) = held_at(path, logical, window, cx) else {
        return false;
    };
    let drawn = window.paint_image(at, at, Default::default(), image, 0, false);
    #[cfg(test)]
    PAINTED.with(|painted| {
        if let (Ok(()), Some(paths)) = (&drawn, painted.borrow_mut().as_mut()) {
            paths.push(path.to_owned());
        }
    });
    let _ = drawn;
    true
}

/// The card uses the same held image and window fallback as its flyout.
pub(crate) fn mark(path: String, logical: f32, ink: gpui::Hsla) -> gpui::AnyElement {
    use gpui::{canvas, prelude::*, px};
    canvas(
        |_, _, _| (),
        move |at, _, window, cx| {
            if !paint(&path, logical, at, window, cx) {
                #[cfg(test)]
                FALLBACKS.with(|painted| {
                    if let Some(marks) = painted.borrow_mut().as_mut() {
                        marks.push((path.clone(), ink));
                    }
                });
                let (key, data) = super::glyphs::mark("app-window");
                super::element::paint_mark(key, data, at, ink, window, cx);
            }
        },
    )
    .size(px(logical))
    .flex_none()
    .into_any_element()
}

impl Shell {
    /// Only the open flyout's visible rows start icon work.
    pub(crate) fn load_app_icons(&mut self, window: &Window, cx: &mut Context<Self>) {
        let (wanted, logical) = visible(&self.menu.menu.borrow());
        load(wanted, logical, window, cx);
    }
}

/// Each host supplies only the icons its application viewport actually renders: a resolved file, or a bare Icon= name the backend index could not place, which takes the themed lookup v0.3.12's MenuRow.qml does.
pub(crate) fn load<T: 'static>(
    wanted: impl IntoIterator<Item = String>,
    logical: f32,
    window: &Window,
    cx: &mut Context<T>,
) {
    let side = pixels(logical, window);
    let (size, scale) = (
        logical.round().max(1.) as u32,
        window.scale_factor().ceil().max(1.) as u32,
    );
    let icons = cx.default_global::<AppIcons>();
    let fresh: Vec<String> = wanted
        .into_iter()
        .filter(|icon| !icon.is_empty())
        .filter(|icon| match icons.held.entry((icon.clone(), side)) {
            Entry::Vacant(pending) => pending.insert(None).is_none(),
            Entry::Occupied(_) => false,
        })
        .collect();
    #[cfg(test)]
    let completion_gate = icons.completion_gate.clone();
    for icon in fresh {
        #[cfg(test)]
        let completion_gate = completion_gate.clone();
        let (name, renderer) = (icon.clone(), cx.svg_renderer());
        let work = cx.background_executor().spawn(async move {
            let path = Icons::session().resolve(&name, size, scale)?;
            Some(quick_look_overlay::raster_run(
                &quick_look_overlay::RasterJob {
                    raster: quick_look_overlay::raster_for_path(&path, renderer),
                    path,
                    size: (side, side),
                    epoch: 0,
                    latest: Arc::new(AtomicU64::new(0)),
                },
            ))
        });
        cx.spawn(async move |_, cx| {
            let Some(quick_look_overlay::RasterOutcome::Raster(image)) = work.await else {
                return;
            };
            #[cfg(test)]
            if let Some(gate) = completion_gate {
                let _ = gate.recv().await;
            }
            cx.update(|cx| {
                cx.default_global::<AppIcons>()
                    .held
                    .insert((icon, side), Some(image));
                cx.refresh_windows();
            });
        })
        .detach();
    }
}
