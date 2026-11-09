//! The completed strip stays with the held pane until its replacement listing lands.
use super::bar::{Frame, Hit, HIT_MIN, PAD_X};
use crate::{state::Shell, window::startup::View};
use gpui::{Context, Pixels, Point, Window};

pub(crate) struct Snapshot {
    pub side: usize,
    pub frame: Frame,
    pub identities: Vec<u64>,
}

#[derive(Clone, Copy)]
pub(crate) struct Press {
    pub side: usize,
    pub identity: Option<u64>,
    pub close: bool,
}

impl Shell {
    pub(crate) fn tab_strip_visible(&self) -> bool {
        if self.strip.view.is_none() && self.slots.tabs.is_some() {
            true
        } else if self.tab_holding() {
            self.strip
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.side == self.focus_side)
        } else {
            self.strip.tabs[self.focus_side].count() > 1
        }
    }

    pub(crate) fn tab_press(&self, position: Point<Pixels>) -> Option<Press> {
        let side = self.strip.snapshot.as_ref()?.side;
        let hit = self.tab_hit(position);
        let (identity, close) = match hit {
            Hit::Tab(index) | Hit::Close(index) => (
                Some(
                    self.strip.tabs[side]
                        .tabs
                        .as_ref()?
                        .items
                        .get(index)?
                        .identity,
                ),
                matches!(hit, Hit::Close(_)),
            ),
            Hit::New => (None, false),
            Hit::Nothing => return None,
        };
        Some(Press {
            side,
            identity,
            close,
        })
    }

    pub(crate) fn tab_pressed_hit(&self, press: Press) -> Hit {
        if press.side != self.focus_side {
            return Hit::Nothing;
        }
        match press.identity {
            Some(identity) => {
                self.strip.tabs[press.side]
                    .resolve(identity)
                    .map_or(Hit::Nothing, |index| {
                        if press.close {
                            Hit::Close(index)
                        } else {
                            Hit::Tab(index)
                        }
                    })
            }
            None => Hit::New,
        }
    }

    pub(crate) fn sync_tab_typeface(&mut self) -> bool {
        let Some(host) = &self.panes[self.focus_side] else {
            return false;
        };
        let typeface = host.list.borrow().typeface.clone();
        let mut labels = self.strip.labels.borrow_mut();
        if labels.typeface.font == typeface.font && labels.typeface.caption == typeface.caption {
            return false;
        }
        labels.typeface = typeface;
        labels.invalidate();
        true
    }

    pub(crate) fn reflow_held_tabs(
        &mut self,
        window: &Window,
        font_changed: bool,
        cx: &mut Context<Self>,
    ) {
        let width = self.geometry(window).tabs.width;
        let caption = self.panes[self.focus_side]
            .as_ref()
            .map(|host| f32::from(host.list.borrow().typeface.caption));
        let Some(snapshot) = &mut self.strip.snapshot else {
            return;
        };
        let Some(caption) = caption else { return };
        self.strip.bar.width = width;
        self.strip.bar.caption = caption;
        if snapshot.frame.icons.is_none() {
            snapshot.frame.tab_width =
                super::bar::tab_width(width, snapshot.frame.labels.len(), caption);
        }
        if let Some(view) = &self.strip.view {
            view.update(cx, |strip, cx| {
                if strip.frame != snapshot.frame || font_changed {
                    strip.frame = snapshot.frame.clone();
                    cx.notify();
                }
            });
        }
    }

    /// Keep the completed strip only while the pane holds old rows; the loading fallback exposes the new tab.
    pub(crate) fn tab_holding(&self) -> bool {
        self.view != View::Dual
            && self.panes[self.focus_side]
                .as_ref()
                .is_some_and(|host| host.pane.busy() && host.pane.swap.holding)
    }

    pub(crate) fn tab_hit(&self, position: Point<Pixels>) -> Hit {
        let Some(snapshot) = &self.strip.snapshot else {
            return Hit::Nothing;
        };
        let frame = &snapshot.frame;
        let x = self.tab_x(position) - PAD_X;
        if x < 0. {
            return Hit::Nothing;
        }
        let slot = (x / frame.tab_width).floor() as usize;
        if slot >= frame.labels.len() {
            return if x < frame.labels.len() as f32 * frame.tab_width + HIT_MIN {
                Hit::New
            } else {
                Hit::Nothing
            };
        }
        let Some(drawn) = frame.slots.iter().position(|index| *index == slot) else {
            return Hit::Nothing;
        };
        let Some(identity) = snapshot.identities.get(drawn) else {
            return Hit::Nothing;
        };
        let Some(index) = self.strip.tabs[snapshot.side].resolve(*identity) else {
            return Hit::Nothing;
        };
        if frame.icons.is_none() && x >= (slot as f32 + 1.) * frame.tab_width - HIT_MIN {
            Hit::Close(index)
        } else {
            Hit::Tab(index)
        }
    }
}
