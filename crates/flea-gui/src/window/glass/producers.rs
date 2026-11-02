//! Explicit ground owners: unknown or inherited grounds retain their opaque floor.
use super::{Mode, Windows};
use crate::palette::Theme;
use gpui::{canvas, fill, prelude::*, App, Bounds, Hsla, IntoElement, Pixels, Window};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Producer {
    Chrome,
    Header,
    Path,
    Tabs,
    Rail,
    Status,
    Listing,
    Body,
    Unknown,
}

impl Producer {
    pub(super) const ALL: [Self; 9] = [
        Self::Chrome,
        Self::Header,
        Self::Path,
        Self::Tabs,
        Self::Rail,
        Self::Status,
        Self::Listing,
        Self::Body,
        Self::Unknown,
    ];
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Header => "header",
            Self::Path => "path",
            Self::Tabs => "tabs",
            Self::Rail => "rail",
            Self::Status => "status",
            Self::Listing => "listing",
            Self::Body => "body",
            Self::Unknown => "unknown",
        }
    }
}

pub(crate) fn requested(window: &Window, cx: &App) -> f32 {
    let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
    let clear = cx
        .try_global::<Windows>()
        .and_then(|book| {
            book.0
                .borrow()
                .get(&window.window_handle().window_id())
                .map(|v| v.clear)
        })
        .unwrap_or(false);
    if clear {
        f32::from(theme.glass.wanted_alpha) / 255.
    } else {
        1.
    }
}

pub(crate) fn chrome_theme(window: &Window, cx: &App) -> Theme {
    let mut theme = cx.try_global::<Theme>().copied().unwrap_or_default();
    if requested(window, cx) < 1. && theme.glass.automatic && theme.glass.mode == Mode::Glassy {
        if let Some(inks) = theme.glass.chrome_inks {
            super::recipe::apply(&mut theme, inks);
        }
    }
    theme
}

/// Resolve glyph ink independently from fills, including selected-row washes.
pub(crate) fn text_ink(color: Hsla, window: &Window, cx: &App) -> Hsla {
    let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
    if requested(window, cx) >= 1. || !theme.glass.automatic || theme.glass.mode != Mode::Glassy {
        return color;
    }
    let Some(inks) = theme.glass.chrome_inks else {
        return color;
    };
    super::recipe::lookup(&theme, inks, color)
}

pub(crate) fn color(owner: Producer, mut color: Hsla, window: &Window, cx: &App) -> Hsla {
    let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
    let known = color == theme.background || color == theme.surface || color == theme.chrome();
    let target = requested(window, cx);
    let overlay = owner == Producer::Rail
        && cx
            .try_global::<Windows>()
            .and_then(|book| {
                book.0
                    .borrow()
                    .get(&window.window_handle().window_id())
                    .map(|v| v.rail_overlay)
            })
            .unwrap_or(false);
    let alpha = if owner == Producer::Unknown || !known || overlay {
        1.
    } else if owner == Producer::Body {
        if target < 1. {
            0.
        } else {
            1.
        }
    } else if matches!(owner, Producer::Listing | Producer::Header) {
        if target < 1. && theme.glass.automatic && theme.glass.mode == Mode::Glassy {
            f32::from(theme.glass.wanted_list_alpha) / 255.
        } else if target < 1. && !theme.glass.automatic {
            target
        } else {
            1.
        }
    } else {
        target
    };
    color.a = alpha;
    color
}

pub(crate) fn paint(
    owner: Producer,
    bounds: Bounds<Pixels>,
    color: Hsla,
    window: &mut Window,
    cx: &App,
) {
    let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
    let color = self::color(owner, color, window, cx);
    window.paint_quad(fill(bounds, color));
    super::samples::record(owner, bounds, color, theme, window, cx);
}

pub(crate) fn backdrop(owner: Producer, color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| paint(owner, bounds, color, window, cx),
    )
    .absolute()
    .size_full()
}

pub(crate) fn observer(owner: Producer, color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| {
            let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
            super::samples::record(
                owner,
                bounds,
                self::color(owner, color, window, cx),
                theme,
                window,
                cx,
            );
        },
    )
    .absolute()
    .size_full()
}

pub(crate) fn observed(window: &Window, cx: &App) -> ([Option<u8>; 9], bool) {
    super::samples::observed(window, cx)
}

/// Apply the root ground behind its children and observe that same paint recipe.
pub(crate) trait BodyGround {
    fn body_ground(self, color: Hsla, window: &Window, cx: &App) -> Self;
}

impl BodyGround for gpui::Div {
    fn body_ground(self, color: Hsla, window: &Window, cx: &App) -> Self {
        self.bg(self::color(Producer::Body, color, window, cx))
            .child(observer(Producer::Body, color))
    }
}

pub(crate) fn status_backdrop(color: Hsla) -> impl IntoElement {
    backdrop(Producer::Status, color)
}
