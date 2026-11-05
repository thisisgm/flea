//! The Trash column header: Name, Original Location and Deleted, with the list header's glass when unified.
use super::pane::TrashPane;
use gpui::{div, prelude::*, px, AnyElement, Hsla, SharedString};

impl TrashPane {
    pub(super) fn header(&self, glassy: bool, background: Hsla, titles: Hsla) -> AnyElement {
        let unified = self.unified;
        let ink = self.paint.borrow().ink;
        let board = self.board();
        let tr = |key: &str| SharedString::from(self.catalog.tr(key));
        let deleted = SharedString::from(crate::navigation::sort::title(
            &self.catalog.tr("trash.header.deleted"),
            true,
        ));
        div()
            .debug_selector(|| "trash-header".into())
            .relative()
            // The list header's glass, as composition draws it over every other folder's header.
            .when(unified && glassy, |header| {
                header.child(div().absolute().top_0().left_0().size_full().child(
                    crate::window::glass::producers::backdrop(
                        crate::window::glass::producers::Producer::Header,
                        background,
                    ),
                ))
            })
            // The list header's wash over that glass: foreground at the header's normal strength.
            .when(unified, |header| {
                header.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .bg(ink.foreground.opacity(crate::list::NORMAL)),
                )
            })
            .h(px(board.header_h))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(board.gap))
            .px(px(board.pad_x))
            .text_size(board.caption)
            .text_color(if unified { titles } else { ink.foreground })
            // The list header centres its titles in the line box below its top padding, not in the whole header.
            .when(unified, |header| header.pt(px(board.header_top)))
            .border_b_1()
            .border_color(ink.hair())
            // The Trash board's own header leaves a mark slot before Name; unified, Name stands above the marks.
            .when(!unified, |header| {
                header.child(div().w(px(board.slot)).flex_none())
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .child(tr("trash.header.name")),
            )
            .child(
                div()
                    .w(px(board.location_w))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(tr("trash.header.location")),
            )
            .child(
                div()
                    .w(px(board.deleted_w))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(deleted),
            )
            .into_any_element()
    }
}
