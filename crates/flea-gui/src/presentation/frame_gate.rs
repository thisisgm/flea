//! Test-only checks of the quads submitted by real mounted surfaces.
use crate::palette::Theme;
use gpui::{Bounds, Pixels, Quad, VisualTestContext};

// A frame taller and wider than any control is a card, and a card has no exception state.
const CARD_MIN_HEIGHT: f32 = 80.;
const CARD_MIN_WIDTH: f32 = 200.;
// The boards draw a disabled control's frame at this opacity.
pub(crate) const DISABLED_OPACITY: f32 = 0.55;

/// Frames one paint draws in a board state; every other frame is muted_frame at full opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct States {
    pub foreground: usize,
    pub accent_frame: usize,
    pub error_on_surface: usize,
    pub disabled_foreground: usize,
    pub disabled_muted: usize,
}

pub(crate) const NONE: States = States {
    foreground: 0,
    accent_frame: 0,
    error_on_surface: 0,
    disabled_foreground: 0,
    disabled_muted: 0,
};

pub(crate) fn theme() -> Theme {
    let theme = Theme::from_text(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/sunset/colors.toml"
    )));
    assert_ne!(
        theme.muted_frame, theme.metadata_frame,
        "fixture must distinguish frame roles"
    );
    theme
}

pub(crate) fn capture(cx: &mut VisualTestContext) -> (f32, Vec<Quad>) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        (window.scale_factor(), window.painted_quads())
    })
}

pub(crate) fn assert_frame(
    name: &str,
    cx: &mut VisualTestContext,
    bounds: Bounds<Pixels>,
    radius: f32,
    theme: Theme,
) {
    let (scale, quads) = capture(cx);
    let own: Vec<_> = quads
        .iter()
        .filter(|quad| quad.bounds == bounds.scale(scale) && quad.border_widths.top.0 > 0.)
        .collect();
    assert!(
        !own.is_empty(),
        "{name}: mounted surface painted no frame at {bounds:?}"
    );
    // CSS and GPUI clamp a circular corner to half the shorter edge.
    let painted_radius = radius
        .min(f32::from(bounds.size.width) / 2.)
        .min(f32::from(bounds.size.height) / 2.);
    let mut failures = Vec::new();
    for quad in own {
        let radii = quad.corner_radii;
        for got in [
            radii.top_left,
            radii.top_right,
            radii.bottom_left,
            radii.bottom_right,
        ] {
            if got.0 / scale != painted_radius {
                failures.push(format!(
                    "radius={} expected={painted_radius}",
                    got.0 / scale
                ));
            }
        }
        if quad.border_color != theme.muted_frame {
            failures.push(format!(
                "ink={:?} expected=muted_frame({:?})",
                quad.border_color, theme.muted_frame
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{name}: painted frame violations: {failures:?}"
    );
    println!("VF_FRAMES surface={name} radius={radius} ink=muted_frame PASS");
}

pub(crate) fn assert_unframed(
    name: &str,
    cx: &mut VisualTestContext,
    bounds: Bounds<Pixels>,
    board: &str,
) {
    let (scale, quads) = capture(cx);
    assert!(
        quads.iter().any(|q| q.bounds == bounds.scale(scale)),
        "{name}: bare surface must paint"
    );
    assert!(
        !quads.iter().any(|q| q.bounds == bounds.scale(scale)
            && q.border_color.a > 0.
            && q.border_widths.top.0 > 0.),
        "{name}: {board} draws no outer frame"
    );
    println!("VF_FRAMES surface={name} unframed={board} PASS");
}

pub(crate) fn assert_inks(name: &str, quads: &[Quad], scale: f32, theme: Theme) {
    let roles = [
        theme.muted_frame,
        theme.foreground,
        theme.accent_frame,
        theme.error_on_surface,
        theme.metadata_frame,
    ];
    assert!(
        roles
            .iter()
            .enumerate()
            .all(|(i, a)| roles[i + 1..].iter().all(|b| a != b)),
        "{name}: the theme must keep every frame role distinct"
    );
    let mut states = NONE;
    let mut count = 0;
    for quad in quads {
        let widths = quad.border_widths;
        if ![widths.top, widths.right, widths.bottom, widths.left]
            .iter()
            .all(|w| w.0 > 0.)
            || quad.border_color.a == 0.
        {
            continue;
        }
        let disabled = quad.border_color.a == DISABLED_OPACITY;
        assert!(
            disabled || quad.border_color.a == 1.,
            "{name}: frame opacity {} is neither full nor disabled",
            quad.border_color.a
        );
        let mut ink = quad.border_color;
        ink.a = 1.;
        let height = quad.bounds.size.height.0 / scale;
        let width = quad.bounds.size.width.0 / scale;
        if height > CARD_MIN_HEIGHT && width > CARD_MIN_WIDTH {
            assert_eq!(
                quad.border_color, theme.muted_frame,
                "{name}: card frame must use muted_frame"
            );
        }
        let slot = if ink == theme.muted_frame && !disabled {
            None
        } else if ink == theme.muted_frame {
            Some(&mut states.disabled_muted)
        } else if ink == theme.foreground && disabled {
            Some(&mut states.disabled_foreground)
        } else if ink == theme.foreground {
            Some(&mut states.foreground)
        } else if ink == theme.accent_frame && !disabled {
            Some(&mut states.accent_frame)
        } else if ink == theme.error_on_surface && !disabled {
            Some(&mut states.error_on_surface)
        } else {
            panic!("{name}: frame ink {:?} has no HIG role", quad.border_color)
        };
        if let Some(slot) = slot {
            *slot += 1;
        }
        count += 1;
    }
    assert!(count > 0, "{name}: no painted frame receipt");
    let expected = super::frame_census::CENSUS
        .iter()
        .find(|(surface, _)| *surface == name)
        .map_or(NONE, |(_, states)| *states);
    assert_eq!(
        states, expected,
        "{name}: board-state frames differ from presentation/frame_census.rs"
    );
}

#[derive(Default)]
pub(crate) struct Marks(pub std::cell::RefCell<Vec<(String, gpui::Hsla)>>);
impl gpui::Global for Marks {}

pub(crate) fn mark(id: &str, color: gpui::Hsla, cx: &gpui::App) {
    if let Some(receipts) = cx.try_global::<Marks>() {
        receipts.0.borrow_mut().push((id.into(), color));
    }
}
