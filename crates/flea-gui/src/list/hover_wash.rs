//! Settings > View > Highlight on hover as one window-wide value: off, no row, tile or rail row washes under the pointer.
use flea::jsondoc::Json;
use gpui::{App, Global};

/// Ticked, the pointer's row, tile and rail row take the 8% foreground wash and lift their ink; a fresh install draws neither.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HoverWash(pub bool);

impl Global for HoverWash {}

impl HoverWash {
    /// Only a stored true ticks it; an absent or unreadable key reads off, as decision hover-default ships it.
    pub fn read(data: &Json) -> Self {
        Self(data.get("highlightHover").and_then(Json::as_bool) == Some(true))
    }

    pub fn on(cx: &App) -> bool {
        cx.try_global::<Self>().is_some_and(|wash| wash.0)
    }

    /// Only a change reaches the global's observers.
    pub fn apply(data: &Json, cx: &mut App) {
        let wash = Self::read(data);
        if cx.try_global::<Self>().copied().unwrap_or_default() != wash {
            cx.set_global(wash);
        }
    }
}

#[cfg(test)]
#[path = "hover_wash_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "hover_surface_tests.rs"]
mod surface_tests;
