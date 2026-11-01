//! Ctrl and the wheel or touchpad walk the thumbnail stops, as GridArea.qml zoomWheel does.
use super::{geometry::ThumbSize, FocusFacts, GridView, Shown};

/// One wheel event under Ctrl: touchpad pixels count when a stroke is in phase or carries no angle, else wheel units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomInput {
    pub touchpad: bool,
    pub pixels: f32,
    pub angle: f32,
}

/// Scroll.js zoomTravel with GridArea.qml's 48 px touchpad step and 120 unit notch.
pub fn zoom_travel(input: ZoomInput) -> f32 {
    if input.pixels != 0. && (input.touchpad || input.angle == 0.) {
        input.pixels / 48.
    } else {
        input.angle / 120.
    }
}

impl GridView {
    /// GridArea.qml zoomWheel: travel accrues and whole steps walk the thumbnail stops; the answer is the size to persist as preview.thumbSize.
    pub fn zoom(
        &mut self,
        shown: Shown,
        focus: &dyn FocusFacts,
        input: ZoomInput,
    ) -> Option<ThumbSize> {
        self.zoom_travel += zoom_travel(input);
        let steps = self.zoom_travel.trunc();
        if steps == 0. {
            return None;
        }
        self.zoom_travel -= steps;
        let next = self.thumb.stepped(steps as i32);
        if next == self.thumb {
            return None;
        }
        self.reflow(shown, focus, |grid| grid.thumb = next);
        Some(next)
    }
}

impl crate::state::Shell {
    pub(crate) fn grid_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let enabled = self
            .launch
            .settings
            .data
            .get("preview")
            .and_then(|v| v.get("ctrlZoom"))
            .and_then(flea::jsondoc::Json::as_bool)
            != Some(false);
        if event.modifiers.control && enabled {
            let input = match event.delta {
                gpui::ScrollDelta::Pixels(delta) => ZoomInput {
                    touchpad: true,
                    pixels: delta.y.into(),
                    angle: 0.,
                },
                gpui::ScrollDelta::Lines(delta) => ZoomInput {
                    touchpad: false,
                    pixels: 0.,
                    angle: delta.y * 40.,
                },
            };
            let next = self.panes[self.focus_side]
                .as_mut()
                .filter(|host| !host.pane.busy())
                .and_then(|host| {
                    let grid = host.grid.as_mut()?;
                    if grid.next_view.is_some() {
                        return None;
                    }
                    grid.view.zoom(
                        Shown::All(host.pane.held.total),
                        &super::host::GridHost::focus(&host.pane),
                        input,
                    )
                });
            if let Some(next) = next {
                use flea::jsondoc::Json;
                self.change_settings(
                    Json::Obj(vec![(
                        "preview".into(),
                        Json::Obj(vec![("thumbSize".into(), Json::Str(next.name().into()))]),
                    )]),
                    window,
                    cx,
                );
            }
            cx.notify();
            return;
        }
        let delta = -f32::from(crate::input::scroll::wheel_delta(&event.delta).y);
        let glide_at = (matches!(event.delta, gpui::ScrollDelta::Lines(_)) && !cx.reduce_motion())
            .then(|| cx.background_executor().now());
        self.grid_scroll_at(delta, glide_at, cx);
    }
}
