//! The painted rail editor registers its existing producer only while it holds the keyboard.
use super::*;

impl RailElement {
    pub(super) fn attach_rename(
        &self,
        model: &RowsModel,
        laid: &Laid,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(rect) = model
            .renaming
            .filter(|_| model.focused && model.editor.editing)
            .and_then(|index| laid.row(index)?.editor)
            .map(box_of)
            .filter(|rect| rect.intersect(&bounds).size.height > px(0.))
        else {
            return;
        };
        if let Some(focus) = window.focused(cx) {
            window.handle_input(
                &focus,
                gpui::ElementInputHandler::new(rect, self.producer.clone()),
                cx,
            );
        }
    }
}
