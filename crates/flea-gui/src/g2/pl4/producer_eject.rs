//! The eject mark: its hit box under the pointer, its press and release, and the keyboard frame Right Arrow reaches.
use super::*;
use gpui::{MouseUpEvent, Pixels, Point};

impl Producer {
    /// The row whose eject mark's hit box holds the point, as SidebarRow.qml's ejectAt reads it.
    pub(crate) fn mark_at(&self, position: Point<Pixels>) -> Option<usize> {
        let (x, y) = (f32::from(position.x), f32::from(position.y));
        self.laid
            .borrow()
            .rows
            .iter()
            .find(|row| {
                row.eject_hit().is_some_and(|hit| {
                    x >= hit.x && x < hit.x + hit.width && y >= hit.y && y < hit.y + hit.height
                })
            })
            .map(|row| row.index)
    }

    /// A left press on a mark arms it; the row underneath never activates for that press.
    pub(super) fn mark_press(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.mark_at(position) else {
            return false;
        };
        self.cancel_rename(cx);
        let mut model = self.model.borrow_mut();
        model.pointer_focus = true;
        model.mark_focused = false;
        model.mark_pressed = Some(index);
        drop(model);
        cx.notify();
        true
    }

    /// TapHandler ReleaseWithinBounds: a release inside the pressed mark's hit box releases its row.
    pub(super) fn mark_release(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(index) = self.model.borrow_mut().mark_pressed.take() else {
            return;
        };
        if self.mark_at(event.position) == Some(index) {
            if let Some(entry) = self.entry(index).filter(|entry| entry.releasable) {
                self.queue(Event::Release(entry));
            }
        }
        cx.notify();
    }

    pub(super) fn mark_cancel(&mut self) {
        self.model.borrow_mut().mark_pressed = None;
    }

    /// RailKeys.markKey: Right reaches a releasable row's mark, Return or Space there releases, Left or Escape leaves.
    pub fn mark_key(&mut self, key: &str, plain: bool, cx: &mut Context<Self>) -> bool {
        if !plain {
            return false;
        }
        let releasable = self
            .cursor()
            .and_then(|index| self.entry(index))
            .filter(|entry| entry.releasable);
        let mut model = self.model.borrow_mut();
        if !model.mark_focused {
            if key != "right" || releasable.is_none() {
                return false;
            }
            model.mark_focused = true;
            model.pointer_focus = false;
        } else {
            match key {
                "left" | "escape" => model.mark_focused = false,
                "enter" | "space" => {
                    model.mark_focused = false;
                    drop(model);
                    if let Some(entry) = releasable {
                        self.queue(Event::Release(entry));
                    }
                    cx.notify();
                    return true;
                }
                "right" => {}
                _ => return false,
            }
        }
        drop(model);
        cx.notify();
        true
    }
}
