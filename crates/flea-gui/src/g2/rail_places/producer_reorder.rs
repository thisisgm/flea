//! Favorites reorder by drag, SidebarRow.qml's DragHandler: a held favourite ghosts, a bar marks where it lands, the release moves it.
use super::*;
use crate::columns::preview_host::START_DRAG_DISTANCE;
use crate::g2::rail_places::rows::Reorder;
use crate::tabs::bar::rail_reorder;
use flea::ui_logic_rail_places::favourites::Operation;

impl Producer {
    /// A left press on a favourite holds it; TapHandler's DragThreshold opens it on a release that never dragged.
    pub(super) fn reorder_press(&mut self, index: usize, entry: &RailEntry, y: f32) -> bool {
        let Some(from) = entry.favourite_index else {
            return false;
        };
        let count = self
            .entries()
            .iter()
            .filter(|entry| entry.favourite_index.is_some())
            .count();
        let mut model = self.model.borrow_mut();
        model.cursor = Some(index);
        model.reorder = Some(Reorder {
            row: index,
            from,
            count,
            press_y: y,
            start_y: None,
            line: from,
        });
        true
    }

    pub(super) fn reorder_move(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        let mut model = self.model.borrow_mut();
        let Some(mut held) = model.reorder else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            model.reorder = None;
            drop(model);
            cx.notify();
            return;
        }
        let y = f32::from(event.position.y);
        if held.start_y.is_none() && (y - held.press_y).abs() > START_DRAG_DISTANCE {
            held.start_y = Some(y);
        }
        // DragHandler's translation counts from the press, so the threshold run is part of the move.
        if held.start_y.is_some() {
            held.line = rail_reorder(
                f64::from(y - held.press_y),
                held.from,
                self.row_height(),
                held.count,
            )
            .line;
        }
        if model.reorder != Some(held) {
            model.reorder = Some(held);
            drop(model);
            cx.notify();
        }
    }

    /// The release: a drag moves the favourite through the store, a still press opens the row.
    pub(super) fn reorder_release(&mut self, y: f32, cx: &mut Context<Self>) {
        let Some(held) = self.model.borrow_mut().reorder.take() else {
            return;
        };
        match held.start_y {
            Some(_) => {
                let to = rail_reorder(
                    f64::from(y - held.press_y),
                    held.from,
                    self.row_height(),
                    held.count,
                )
                .to;
                if to != held.from {
                    self.write_favourite(
                        &Operation::Move {
                            index: held.from,
                            to,
                        },
                        cx,
                    );
                }
                cx.notify();
            }
            None => self.activate(held.row, cx),
        }
    }

    // Theme.railRowHeight, as the rows were laid.
    fn row_height(&self) -> f64 {
        let laid = self.laid.borrow();
        f64::from(laid.rows.first().map_or(1., |row| row.rect.height.max(1.)))
    }
}
