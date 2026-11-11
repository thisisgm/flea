//! A held rail drag aimed again when the rail's rows change under it (RailDrop.qml onEntriesChanged).
use super::*;

impl Shell {
    /// RailDrop.qml onEntriesChanged: a held drag whose rail row now sends it elsewhere is aimed again at the resting pointer.
    pub(crate) fn reaim_rail_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = window_key(window);
        let point = window.mouse_position();
        let Some(drags) = cx.try_global::<Drags>() else {
            return;
        };
        let Some(lift) = drags.lifts.get(&key) else {
            return;
        };
        let aimed = match drags.aims.get(&key) {
            Some(aim) => match aim.spot {
                Spot::Rail(row) => Some((row, aim.side, aim.place.clone())),
                Spot::Row(_) | Spot::Tab(_) => return,
            },
            None => None,
        };
        let current = self
            .rail_target(point, lift, cx)
            .map(|(row, side, _, place)| (row, side, place));
        if current == aimed || !cx.has_active_drag() {
            return;
        }
        self.aim_rail_drag(point, window, cx);
        window.request_animation_frame();
    }
}
