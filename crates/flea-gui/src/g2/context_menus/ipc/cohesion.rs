//! The menu card's built rows, clipped to the card that was painted.
use super::MenuAdapter;
use flea::jsondoc::Json;

impl MenuAdapter<'_> {
    pub(super) fn cohesion(&self, pointer: Option<(f32, f32)>) -> Option<String> {
        let card = self.menu.card.as_ref()?;
        let entries = card
            .built
            .lines
            .iter()
            .enumerate()
            .filter_map(|(at, line)| {
                if line.separator {
                    return None;
                }
                let top = card.row_screen_top(at);
                let height = card.built.metrics.height(card.entries.get(at)?) as f32;
                let y = top.max(card.frame.y + card.built.metrics.pad as f32);
                let end = (top + height)
                    .min(card.frame.y + (card.built.metrics.pad + card.view_h) as f32);
                if end <= y {
                    return None;
                }
                let x = card.frame.x + self.origin.0;
                let clipped_height = end - y;
                let y = y + self.origin.1;
                let hovered = pointer.is_some_and(|(x, y)| card.row_at(x, y) == Some(at));
                let source = crate::visual::object([
                    ("name", Json::Str(super::label(line).into())),
                    ("value", Json::Str(line.action.clone())),
                    (
                        "rect",
                        Json::Str(crate::ipc::rows::rect_of(
                            x,
                            y,
                            card.frame.w,
                            clipped_height,
                        )),
                    ),
                    (
                        "centre",
                        Json::Str(self.centre(Some(card), &at.to_string())),
                    ),
                    ("visible", Json::Bool(true)),
                    ("enabled", Json::Bool(!line.disabled)),
                    ("focused", Json::Bool(card.cursor == Some(at))),
                    ("hovered", Json::Bool(hovered)),
                    (
                        "destructive",
                        Json::Bool(line.ink == super::super::lines::Ink::Danger),
                    ),
                ]);
                crate::ipc::cohesion_controls::control(&source, "ContextMenu", "MenuRow")
            })
            .collect();
        Some(
            flea::jsondoc::render(&crate::ipc::cohesion_controls::state(entries))
                .trim_end()
                .into(),
        )
    }
}
