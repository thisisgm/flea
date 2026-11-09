//! Archive mounting for the Quick Look document factory.
use super::Look;
use crate::g2::{
    doc_preview::archive::{ArchiveView, Metrics, Palette},
    quick_look_overlay::look as card,
};
use crate::window::quick_look::wire::Wire;
use flea::i18n::I18n;
use gpui::{px, Context};
use std::sync::Arc;

pub(super) fn mount(
    look: &Look,
    wire: &Wire,
    catalog: &Arc<I18n>,
    path: &str,
    cx: &mut Context<ArchiveView>,
) -> ArchiveView {
    let folder = crate::row::marks::Mark::Folder.svg();
    let file = crate::row::marks::Mark::File.svg();
    let mut view = ArchiveView::new(
        Palette {
            foreground: look.theme.foreground,
            muted: look.muted,
            muted_frame: look.theme.muted_frame,
        },
        Metrics {
            inset: card::PAD,
            gap: card::GAP,
            row: 22.,
            glyph: card::CAPTION,
            font: look.face.font.clone(),
            name: px(card::BODY),
            caption: px(card::CAPTION),
            more_opacity: 0.6,
        },
        look.mark.clone(),
        file,
        folder,
    );
    view.set_host(Some(Box::new(wire.clone())));
    view.set_catalog(catalog.clone(), cx);
    view.set_subject(path, cx);
    view.set_active(true, cx);
    view
}
