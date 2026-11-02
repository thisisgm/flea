//! Ancestor drops use the painted crumb advances and the ordinary device-aware drop decision.
use super::*;
use crate::path::{paint::strip_pad, strip::Hit};

impl Shell {
    pub(super) fn file_crumb_hit(
        &self,
        point: Point<gpui::Pixels>,
        cx: &App,
    ) -> Option<(usize, usize, usize, String)> {
        let chrome = self.chrome.view.as_ref()?.read(cx);
        let view = chrome.path.read(cx);
        if view.inputs.blocked || view.editing() {
            return None;
        }
        let paint = view.paint.borrow();
        let slots: &[usize] = if view.inputs.dual { &[1, 2] } else { &[0] };
        slots.iter().find_map(|&slot| {
            let bounds = paint.strips[slot]?;
            if !bounds.contains(&point) {
                return None;
            }
            let strip = &view.strips[slot];
            let x = f32::from(point.x - bounds.left()) - strip_pad(slot != 0, &paint.scale());
            let Hit::Crumb(index) = strip.hit(x) else {
                return None;
            };
            let crumb = strip.crumbs().get(index)?;
            (crumb.parent() && !crumb.path.is_empty()).then(|| {
                (
                    if slot == 0 {
                        view.inputs.side
                    } else {
                        slot - 1
                    },
                    slot,
                    index,
                    crumb.path.clone(),
                )
            })
        })
    }

    pub(super) fn file_crumb_destination(
        &self,
        point: Point<gpui::Pixels>,
        cx: &App,
    ) -> Option<(usize, String)> {
        self.file_crumb_hit(point, cx)
            .map(|(side, _, _, path)| (side, path))
    }

    pub(super) fn show_crumb_drop(
        &mut self,
        target: Option<(usize, usize)>,
        cx: &mut Context<Self>,
    ) {
        let target = target.filter(|_| !self.input_blocked(cx));
        if let Some(chrome) = &self.chrome.view {
            let path = chrome.read(cx).path.clone();
            path.update(cx, |view, cx| {
                if view.drop_target != target {
                    view.drop_target = target;
                    cx.notify();
                }
            });
        }
    }

    pub(super) fn land_crumb(
        &mut self,
        side: usize,
        dest: String,
        owned: Owned,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_crumb_drop(None, cx);
        if self.panes[side]
            .as_ref()
            .is_some_and(|host| host.pane.busy())
        {
            self.refuse_drop_loading(side, cx);
        } else {
            self.land_then(side, super::wide::Then::Into(dest), owned, window, cx);
        }
    }
}
