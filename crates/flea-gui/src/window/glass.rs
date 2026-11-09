//! Startup surface identity and truthful observations of each mounted window.
use super::glass_tag::Mode;
use crate::{ipc::Adapter, palette::Theme};
use flea::{
    ipc::wire::Reply,
    theme::{parse_shell, Material},
};
use gpui::{App, Global, Window, WindowBackgroundAppearance, WindowId};
use std::cell::RefCell;
use std::collections::HashMap;
pub(crate) mod producers;
mod recipe;
mod samples;

// WCAG body text contrast ratio.
const TEXT_CONTRAST: f64 = 4.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inputs {
    pub mode: Mode,
    pub automatic: bool,
    pub pins_opaque: bool,
    pub wanted_alpha: u8,
    pub wanted_list_alpha: u8,
    pub chrome_inks: Option<[gpui::Hsla; 12]>,
}

impl Default for Inputs {
    fn default() -> Self {
        Self {
            mode: Mode::Transparent,
            automatic: true,
            pins_opaque: false,
            wanted_alpha: 255,
            wanted_list_alpha: 255,
            chrome_inks: None,
        }
    }
}

fn value(text: &str) -> Option<u8> {
    let values = parse_shell(text);
    let raw = values.get("flea.background-alpha")?;
    let number = raw
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .unwrap_or(1.);
    Some((number.clamp(0., 1.) * 255.).round() as u8)
}

#[cfg(test)]
thread_local! {
    // Each Glassy alpha search a test thread ran, with its time, as tests/material-startup.sh counts the QML oracle's.
    static FLOOR_SEARCHES: RefCell<Vec<std::time::Duration>> = const { RefCell::new(Vec::new()) };
}

// The automatic Glassy alpha: the lowest that keeps body text at WCAG contrast over the material.
fn floor(material: &Material) -> u8 {
    #[cfg(test)]
    let started = std::time::Instant::now();
    let alpha = material.floor(TEXT_CONTRAST);
    #[cfg(test)]
    FLOOR_SEARCHES.with_borrow_mut(|searches| searches.push(started.elapsed()));
    alpha
}

impl Inputs {
    pub(crate) fn resolve(
        colors: &str,
        theme: &str,
        user: &str,
        opaque: bool,
        glassy: bool,
    ) -> Self {
        let (user, theme) = (value(user), value(theme));
        let mode = Mode::from_toggles(glassy, opaque);
        let automatic = user.is_none() && theme.is_none();
        let core = flea::theme::Theme::resolve(colors, "", "", None, "monospace");
        let material = Material::from_colors(&core.colors, true);
        let alpha = user.unwrap_or_else(|| match mode {
            Mode::Opaque => 255,
            Mode::Glassy => theme.unwrap_or_else(|| floor(&material)),
            Mode::Transparent => theme.unwrap_or(255),
        });
        let list_alpha = alpha;
        Self {
            mode,
            automatic,
            pins_opaque: user.or(theme) == Some(255),
            wanted_alpha: alpha,
            wanted_list_alpha: list_alpha,
            chrome_inks: (automatic && mode == Mode::Glassy)
                .then(|| recipe::inks(&core.colors, alpha)),
        }
    }

    #[cfg(test)]
    pub(crate) fn take_floor_searches() -> Vec<std::time::Duration> {
        FLOOR_SEARCHES.take()
    }

    pub(crate) fn appearance(self) -> WindowBackgroundAppearance {
        if self.pins_opaque {
            WindowBackgroundAppearance::Opaque
        } else {
            WindowBackgroundAppearance::Transparent
        }
    }
}

#[derive(Clone)]
struct Mounted {
    clear: bool,
    rail_overlay: bool,
    samples: samples::Samples,
}

#[derive(Default)]
struct Windows(RefCell<HashMap<WindowId, Mounted>>);
impl Global for Windows {}

#[cfg(test)]
pub(crate) fn producer_window_count(cx: &App) -> usize {
    cx.try_global::<Windows>()
        .map_or(0, |book| book.0.borrow().len())
}

pub(crate) fn mount(window: &Window, appearance: WindowBackgroundAppearance, cx: &mut App) {
    if cx.try_global::<Windows>().is_none() {
        cx.set_global(Windows::default());
        cx.on_window_closed(|cx, id| {
            cx.global::<Windows>().0.borrow_mut().remove(&id);
        })
        .detach();
    }
    cx.global::<Windows>().0.borrow_mut().insert(
        window.window_handle().window_id(),
        Mounted {
            clear: appearance != WindowBackgroundAppearance::Opaque,
            rail_overlay: false,
            samples: Vec::new(),
        },
    );
}

pub(crate) struct Reader<'a> {
    pub window: &'a Window,
    pub cx: &'a App,
}

impl Adapter for Reader<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        if function != "glass" {
            return None;
        }
        let Some(mounted) = self.cx.try_global::<Windows>().and_then(|windows| {
            windows
                .0
                .borrow()
                .get(&self.window.window_handle().window_id())
                .cloned()
        }) else {
            return Some(Reply::error(
                3,
                "FLEA_IPC_UNAVAILABLE flea.glass: window not mounted",
            ));
        };
        let theme = producers::chrome_theme(self.window, self.cx);
        let inputs = theme.glass;
        let mode = match inputs.mode {
            Mode::Opaque => "opaque",
            Mode::Glassy => "glassy",
            Mode::Transparent => "transparent",
        };
        // Window identity and producer paint samples are independent observations.
        let window = if mounted.clear {
            "#00000000".into()
        } else {
            crate::ipc::rows::colour(theme.background)
        };
        let alpha = alpha(self.window, self.cx);
        let (painted, ready) = producers::observed(self.window, self.cx);
        let list_alpha =
            f32::from(painted[producers::Producer::Listing as usize].unwrap_or(255)) / 255.;
        let grounds = producers::Producer::ALL
            .iter()
            .map(|&p| {
                format!(
                    "\"{}\":{}",
                    p.name(),
                    painted[p as usize].map_or("null".into(), |a| a.to_string())
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let text_roles = [
            ("foreground", theme.foreground),
            ("muted", theme.muted),
            ("metadata", theme.metadata),
            ("metadataFrame", theme.metadata_frame),
            ("mutedFrame", theme.muted_frame),
        ]
        .into_iter()
        .map(|(name, color)| {
            format!(
                "\"{name}\":{{\"color\":\"{}\",\"alpha\":{}}}",
                crate::ipc::rows::colour(color),
                color.a
            )
        })
        .collect::<Vec<_>>()
        .join(",");
        // Actual producer paint samples stay distinct from the requested theme alphas.
        Some(Reply::value(format!(
            "{{\"clear\":{},\"mode\":\"{mode}\",\"focused\":{},\"automatic\":{},\"alpha\":{alpha},\"listAlpha\":{list_alpha},\"foreground\":\"{}\",\"muted\":\"{}\",\"metadata\":\"{}\",\"window\":\"{window}\",\"early\":{},\"material\":{},\"groundReady\":{ready},\"grounds\":{{{grounds}}},\"textRoles\":{{{text_roles}}},\"requestedAlpha\":{},\"requestedListAlpha\":{}}}",
            mounted.clear, self.window.is_window_active(), inputs.automatic,
            crate::ipc::rows::colour(theme.foreground), crate::ipc::rows::colour(theme.muted),
            crate::ipc::rows::colour(theme.metadata), mounted.clear, ready && alpha < 1. && inputs.automatic && inputs.mode == Mode::Glassy,
            f64::from(inputs.wanted_alpha) / 255., f64::from(inputs.wanted_list_alpha) / 255.,
        )))
    }
}

pub(crate) fn alpha(window: &Window, cx: &App) -> f32 {
    let mounted = cx.try_global::<Windows>().and_then(|windows| {
        windows
            .0
            .borrow()
            .get(&window.window_handle().window_id())
            .cloned()
    });
    let Some(mounted) = mounted.filter(|m| m.clear) else {
        return 1.;
    };
    let (samples, ready) = producers::observed(window, cx);
    if !ready {
        return 1.;
    }
    let actual = producers::Producer::ALL
        .iter()
        .filter(|&&p| {
            p != producers::Producer::Listing
                && p != producers::Producer::Header
                && !(p == producers::Producer::Rail && mounted.rail_overlay)
        })
        .filter_map(|&p| samples[p as usize])
        .max()
        .unwrap_or(255);
    f32::from(actual) / 255.
}

pub(crate) fn ground(color: gpui::Hsla, window: &Window, cx: &App) -> gpui::Hsla {
    producers::color(producers::Producer::Tabs, color, window, cx)
}

pub(crate) fn rail_overlay(window: &Window, overlay: bool, cx: &mut App) {
    if cx.try_global::<Windows>().is_some() {
        let windows = cx.global::<Windows>();
        if let Some(mounted) = windows
            .0
            .borrow_mut()
            .get_mut(&window.window_handle().window_id())
        {
            mounted.rail_overlay = overlay;
        }
    }
}

pub(crate) fn tab_ground(
    bounds: gpui::Bounds<gpui::Pixels>,
    current: Option<gpui::Bounds<gpui::Pixels>>,
    window: &mut Window,
    cx: &App,
) {
    let color = cx
        .try_global::<Theme>()
        .copied()
        .unwrap_or_default()
        .surface;
    if producers::requested(window, cx) >= 1. {
        producers::paint(producers::Producer::Tabs, bounds, color, window, cx);
        return;
    }
    let color = cx
        .try_global::<Theme>()
        .copied()
        .unwrap_or_default()
        .surface;
    if let Some(tab) = current {
        let left = tab.left().max(bounds.left()).min(bounds.right());
        let right = tab.right().max(left).min(bounds.right());
        producers::paint(
            producers::Producer::Tabs,
            gpui::Bounds::from_corners(bounds.origin, gpui::point(left, bounds.bottom())),
            color,
            window,
            cx,
        );
        producers::paint(
            producers::Producer::Tabs,
            gpui::Bounds::from_corners(gpui::point(right, bounds.top()), bounds.bottom_right()),
            color,
            window,
            cx,
        );
    } else {
        producers::paint(producers::Producer::Tabs, bounds, color, window, cx);
    }
}

pub(crate) struct Grounds {
    pub glassy: bool,
    pub planes: [gpui::Hsla; 2],
    pub focus_side: usize,
}

impl Grounds {
    pub(crate) fn path_fallback(
        &self,
        side: usize,
        supplied: bool,
    ) -> Option<impl gpui::IntoElement> {
        let color = self.planes[usize::from(side == self.focus_side)];
        (self.glassy && !supplied).then(|| producers::backdrop(producers::Producer::Path, color))
    }

    pub(crate) fn chrome(&self) -> impl gpui::IntoElement {
        producers::backdrop(producers::Producer::Chrome, self.planes[1])
    }

    pub(crate) fn header(&self) -> impl gpui::IntoElement {
        producers::backdrop(producers::Producer::Header, self.planes[0])
    }

    pub(crate) fn status_backdrop(&self) -> impl gpui::IntoElement {
        producers::backdrop(producers::Producer::Status, self.planes[1])
    }

    pub(crate) fn listing(color: gpui::Hsla) -> impl gpui::IntoElement {
        producers::backdrop(producers::Producer::Listing, color)
    }

    pub(crate) fn for_window(window: &Window, focus_side: usize, cx: &App) -> Self {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        Self {
            glassy: producers::requested(window, cx) < 1.,
            planes: [theme.background, theme.surface],
            focus_side,
        }
    }
}
