//! dragState: this window's own file drag while it is in flight, and the folder a drop at the pointer would land in.
use super::Adapter;
use crate::state::Shell;
use crate::visual::{number, object};
use flea::ipc::wire::Reply;
use flea::jsondoc::Json;
use gpui::{App, Window};

pub(crate) struct DragState<'a> {
    pub shell: &'a Shell,
    pub window: &'a Window,
    pub cx: &'a App,
}

impl DragState<'_> {
    fn state(&self) -> Json {
        let Some(lift) = self.shell.file_drag_lift(self.window, self.cx) else {
            return object([("active", Json::Bool(false))]);
        };
        let pointer = self.window.mouse_position();
        let at = (f32::from(pointer.x), f32::from(pointer.y));
        // The pane under the pointer resolves the folder exactly as a drop there would; another pane's lift carries no rows.
        let target = (0..self.shell.panes.len()).find_map(|side| {
            let host = self.shell.panes[side].as_ref()?;
            host.input
                .frame
                .filter(|frame| frame.bounds.contains(&pointer))?;
            let carried = if lift.side == side && lift.listing == host.pane.held.listing {
                lift.rows.as_slice()
            } else {
                &[]
            };
            Some((
                side,
                crate::g2::dg2::join::pane_spot(host, pointer, carried),
            ))
        });
        let paths = lift.paths.as_ref().map_or(Json::Null, |paths| {
            Json::Arr(paths.iter().cloned().map(Json::Str).collect())
        });
        object([
            ("active", Json::Bool(true)),
            ("side", number(lift.side)),
            ("count", number(lift.rows.len())),
            ("paths", paths),
            ("travelled", Json::Bool(lift.travelled(at))),
            (
                "pointer",
                Json::Str(format!("{} {}", at.0.round(), at.1.round())),
            ),
            (
                "overSide",
                target.as_ref().map_or(Json::Null, |(side, _)| number(side)),
            ),
            (
                "target",
                target.map_or(Json::Null, |(_, folder)| Json::Str(folder)),
            ),
        ])
    }
}

impl Adapter for DragState<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        (function == "dragState")
            .then(|| Reply::value(flea::jsondoc::render(&self.state()).trim_end()))
    }
}
