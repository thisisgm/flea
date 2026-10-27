//! Startup surface identity and truthful observations of each mounted window.
use super::glass_tag::Mode;
use crate::{ipc::Adapter, palette::Theme};
use flea::{
    ipc::wire::Reply,
    theme::{parse_shell, Material},
};
use gpui::{App, Global, Window, WindowBackgroundAppearance, WindowId};
use std::collections::HashMap;

// WCAG body text contrast ratio.
const TEXT_CONTRAST: f64 = 4.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inputs {
    pub mode: Mode,
    pub automatic: bool,
    pub pins_opaque: bool,
    pub wanted_alpha: u8,
    pub wanted_list_alpha: u8,
}

impl Default for Inputs {
    fn default() -> Self {
        Self {
            mode: Mode::Transparent,
            automatic: true,
            pins_opaque: false,
            wanted_alpha: 255,
            wanted_list_alpha: 255,
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
            Mode::Glassy => theme.unwrap_or_else(|| material.floor(TEXT_CONTRAST)),
            Mode::Transparent => theme.unwrap_or(255),
        });
        let mut list_alpha = alpha;
        if automatic && mode == Mode::Glassy && alpha < 255 {
            let mut list = material;
            list.surface = list.background;
            let mut floor = 255;
            while floor > 0
                && list
                    .worst(list.muted, floor - 1)
                    .min(list.worst(list.metadata, floor - 1))
                    >= TEXT_CONTRAST
            {
                floor -= 1;
            }
            list_alpha = alpha.max(floor);
        }
        Self {
            mode,
            automatic,
            pins_opaque: user.or(theme) == Some(255),
            wanted_alpha: alpha,
            wanted_list_alpha: list_alpha,
        }
    }

    pub(crate) fn appearance(self) -> WindowBackgroundAppearance {
        if self.pins_opaque {
            WindowBackgroundAppearance::Opaque
        } else {
            WindowBackgroundAppearance::Transparent
        }
    }
}

#[derive(Clone, Copy)]
struct Mounted {
    clear: bool,
    rail_overlay: bool,
}

#[derive(Default)]
struct Windows(HashMap<WindowId, Mounted>);
impl Global for Windows {}

pub(crate) fn mount(window: &Window, appearance: WindowBackgroundAppearance, cx: &mut App) {
    if cx.try_global::<Windows>().is_none() {
        cx.set_global(Windows::default());
        cx.on_window_closed(|cx, id| {
            cx.global_mut::<Windows>().0.remove(&id);
        })
        .detach();
    }
    cx.global_mut::<Windows>().0.insert(
        window.window_handle().window_id(),
        Mounted {
            clear: appearance != WindowBackgroundAppearance::Opaque,
            rail_overlay: false,
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
        let Some(mounted) = self
            .cx
            .try_global::<Windows>()
            .and_then(|windows| windows.0.get(&self.window.window_handle().window_id()))
        else {
            return Some(Reply::error(
                3,
                "FLEA_IPC_UNAVAILABLE flea.glass: window not mounted",
            ));
        };
        let theme = self.cx.try_global::<Theme>().copied().unwrap_or_default();
        let inputs = theme.glass;
        let mode = match inputs.mode {
            Mode::Opaque => "opaque",
            Mode::Glassy => "glassy",
            Mode::Transparent => "transparent",
        };
        // Independent producer floors are opaque until their owners connect the ground hooks.
        let window = if mounted.clear {
            "#00000000".into()
        } else {
            crate::ipc::rows::colour(theme.background)
        };
        let alpha = alpha(self.window, self.cx);
        // listAlpha is the List and Columns floor, which this unit keeps opaque.
        Some(Reply::value(format!(
            "{{\"clear\":{},\"mode\":\"{mode}\",\"focused\":{},\"automatic\":{},\"alpha\":{alpha},\"listAlpha\":1,\"foreground\":\"{}\",\"muted\":\"{}\",\"metadata\":\"{}\",\"window\":\"{window}\",\"early\":{},\"material\":false,\"requestedAlpha\":{},\"requestedListAlpha\":{}}}",
            mounted.clear, self.window.is_window_active(), inputs.automatic,
            crate::ipc::rows::colour(theme.foreground), crate::ipc::rows::colour(theme.muted),
            crate::ipc::rows::colour(theme.metadata), mounted.clear,
            f64::from(inputs.wanted_alpha) / 255., f64::from(inputs.wanted_list_alpha) / 255.,
        )))
    }
}

pub(crate) fn alpha(window: &Window, cx: &App) -> f32 {
    let clear = cx
        .try_global::<Windows>()
        .and_then(|windows| windows.0.get(&window.window_handle().window_id()))
        .is_some_and(|mounted| mounted.clear);
    let inputs = cx.try_global::<Theme>().copied().unwrap_or_default().glass;
    if !clear || (inputs.automatic && inputs.mode == Mode::Glassy) {
        1.
    } else {
        f32::from(inputs.wanted_alpha) / 255.
    }
}

pub(crate) fn ground(mut color: gpui::Hsla, window: &Window, cx: &App) -> gpui::Hsla {
    color.a *= alpha(window, cx);
    color
}

pub(crate) fn body_ground(mut color: gpui::Hsla, window: &Window, cx: &App) -> gpui::Hsla {
    if alpha(window, cx) < 1. {
        color.a = 0.;
    }
    color
}

pub(crate) fn rail_overlay(window: &Window, overlay: bool, cx: &mut App) {
    if cx.try_global::<Windows>().is_some() {
        let windows = cx.global_mut::<Windows>();
        if let Some(mounted) = windows.0.get_mut(&window.window_handle().window_id()) {
            mounted.rail_overlay = overlay;
        }
    }
}

pub(crate) fn rail_ground(color: gpui::Hsla, window: &Window, cx: &App) -> gpui::Hsla {
    if cx
        .try_global::<Windows>()
        .and_then(|windows| windows.0.get(&window.window_handle().window_id()))
        .is_some_and(|mounted| mounted.rail_overlay)
    {
        color
    } else {
        ground(color, window, cx)
    }
}

pub(crate) fn tab_ground(
    bounds: gpui::Bounds<gpui::Pixels>,
    current: Option<gpui::Bounds<gpui::Pixels>>,
    color: gpui::Hsla,
    window: &mut Window,
    cx: &App,
) {
    if alpha(window, cx) >= 1. {
        window.paint_quad(gpui::fill(bounds, color));
        return;
    }
    let color = ground(
        cx.try_global::<Theme>()
            .copied()
            .unwrap_or_default()
            .surface,
        window,
        cx,
    );
    if let Some(tab) = current {
        let left = tab.left().max(bounds.left()).min(bounds.right());
        let right = tab.right().max(left).min(bounds.right());
        window.paint_quad(gpui::fill(
            gpui::Bounds::from_corners(bounds.origin, gpui::point(left, bounds.bottom())),
            color,
        ));
        window.paint_quad(gpui::fill(
            gpui::Bounds::from_corners(gpui::point(right, bounds.top()), bounds.bottom_right()),
            color,
        ));
    } else {
        window.paint_quad(gpui::fill(bounds, color));
    }
}

pub(crate) struct Grounds {
    pub glassy: bool,
    pub planes: [gpui::Hsla; 2],
    pub focus_side: usize,
}

impl Grounds {
    pub(crate) fn path(&self, side: usize) -> gpui::Hsla {
        self.planes[usize::from(side == self.focus_side)]
    }

    pub(crate) fn for_window(window: &Window, focus_side: usize, cx: &App) -> Self {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        Self {
            glassy: alpha(window, cx) < 1.,
            planes: [
                ground(theme.background, window, cx),
                ground(theme.surface, window, cx),
            ],
            focus_side,
        }
    }
}
