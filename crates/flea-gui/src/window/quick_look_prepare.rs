//! The rested cursor's bounded local Markdown entry, invalidated by its row, style or storage.
use super::*;
use crate::{
    backend::Event,
    g2::quick_look_overlay::doc::{Doc, View},
};
use flea::ui_logic_doc_preview::markdown::markdown_prepared as prepared;

impl Shell {
    pub(crate) fn quick_prepare_storage(&mut self, side: usize, event: &Event) {
        let (path, class) = match event {
            Event::FilesystemDetails { facts, class } => (&facts.path, class),
            Event::ListDetails(crate::list::sizes::Reply::Filesystem { path, class }) => {
                (path, class)
            }
            _ => return,
        };
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        if path.is_empty() || path == &host.path {
            host.look_warm.storage = Some((host.path.clone(), class.clone()));
        }
    }

    pub(super) fn arm_quick_prepare(
        &mut self,
        side: usize,
        key: &Key,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        let lent = host.look_warm.lent.take();
        let row = prepared::Row {
            name: &key.row.name,
            dir: key.row.dir,
            mode: key.row.mode,
            size: key.row.size,
        };
        if !prepared::reads_inline(
            Some(&row),
            key.storage.as_deref().unwrap_or(""),
            key.storage.is_some(),
        ) {
            return;
        }
        // The card closed on this same key: the view it showed was read and parsed once, and is the entry.
        let adopted = lent.and_then(|(doc, lent)| {
            let View::Markdown(view) = &doc.view else {
                return None;
            };
            let view = view.clone();
            let shown = &view.read(cx).doc;
            let ready =
                shown.complete && shown.status == crate::g2::doc_preview::markdown::Status::Ready;
            (lent == *key && ready).then_some((view, doc))
        });
        if let Some((view, doc)) = adopted {
            view.update(cx, |view, _| view.park());
            let watch = cx.observe(&view, |_, _, cx| cx.notify());
            if let Some(host) = &mut self.panes[side] {
                host.look_warm.prepared = Some(doc);
                host.look_warm.prepared_watch = Some(watch);
            }
            return;
        }
        let key = key.clone();
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        host.look_warm.prepare_timer = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(120))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if !shell.warm_matches(side, &key, cx) {
                    return;
                }
                let look = super::super::docs::Look {
                    theme: key.theme,
                    face: key.face.clone(),
                    muted: crate::list::Palette::inherited(&window.text_style(), key.theme)
                        .caption(false),
                    mark: super::super::mount::mark(&key.row),
                };
                let path = crate::navigation::history::join(&key.directory, &key.row.name);
                let viewport = window.viewport_size();
                let (width, height) = (
                    f64::from(f32::from(viewport.width)),
                    f64::from(f32::from(viewport.height)),
                );
                let rect = flea::ui_logic_preview_core::rect::surface(
                    &Shape::unknown(State::Text),
                    width,
                    height,
                    false,
                    flea::ui_logic_preview_core::rect::TOKENS,
                );
                let frame = (
                    rect.w as f32,
                    rect.h as f32 - crate::g2::quick_look_overlay::look::BAR,
                );
                let view = cx.new(|cx| {
                    look.markdown(&path, None, &shell.locale.catalog, Some(frame), window, cx)
                });
                let watch = cx.observe(&view, |_, _, cx| cx.notify());
                let Some(host) = &mut shell.panes[side] else {
                    return;
                };
                let warm = &mut host.look_warm;
                warm.prepared = Some(Doc {
                    view: View::Markdown(view),
                    path,
                    page: 0,
                });
                warm.prepared_watch = Some(watch);
                warm.prepare_timer = None;
                cx.notify();
            });
        }));
    }

    /// The open card's Markdown view and the key it shows, so the close can hand it back; the card keeps it built.
    pub(crate) fn lend_quick_prepared(
        &mut self,
        side: usize,
        index: usize,
        row: &RowFacts,
        doc: &Doc,
        cx: &mut Context<Self>,
    ) {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let face = self.active_typeface();
        let locale = self.locale.revision;
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        let storage = host
            .look_warm
            .storage
            .as_ref()
            .filter(|(path, _)| path == &host.path)
            .map(|(_, class)| class.clone());
        let lent = matches!(doc.view, View::Markdown(_)).then(|| {
            let key = Key {
                directory: host.path.clone(),
                listing: host.pane.held.listing,
                cursor: index,
                row: row.clone(),
                theme,
                face,
                locale,
                storage,
            };
            (doc.clone(), key)
        });
        let kept = lent.as_ref().map(|(doc, _)| doc.id());
        host.look_warm.lent = lent;
        if let Some(card) = &self.quick_look {
            card.view.update(cx, |view, _| view.kept = kept);
        }
    }

    pub(crate) fn take_quick_prepared(
        &mut self,
        side: usize,
        row: &RowFacts,
        path: &str,
        cx: &App,
    ) -> Option<Doc> {
        let key = self.panes[side].as_ref()?.look_warm.key.as_ref()?;
        if key.row != *row || !self.warm_matches(side, key, cx) {
            return None;
        }
        let warm = &mut self.panes[side].as_mut()?.look_warm;
        let doc = warm.prepared.as_ref().filter(|doc| doc.path == path)?;
        let View::Markdown(view) = &doc.view else {
            return None;
        };
        let view = view.read(cx);
        if !view.doc.complete || view.doc.status != crate::g2::doc_preview::markdown::Status::Ready
        {
            return None;
        }
        warm.prepared_watch = None;
        warm.prepared.take()
    }
}
