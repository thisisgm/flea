//! The rested cursor's bounded local Markdown entry, invalidated by its row, style or storage.
use super::*;
use crate::{
    backend::Event,
    g2::pv1::doc::{Doc, View},
};
use flea::ui_logic_pv2::markdown::markdown_prepared as prepared;

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
        let key = key.clone();
        self.panes[side].as_mut().unwrap().look_warm.prepare_timer =
            Some(cx.spawn_in(window, async move |this, cx| {
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
                            .metadata,
                        mark: super::super::mount::mark(&key.row),
                    };
                    let path = crate::navigation::history::join(&key.directory, &key.row.name);
                    let viewport = window.viewport_size();
                    let (width, height) = (
                        f64::from(f32::from(viewport.width)),
                        f64::from(f32::from(viewport.height)),
                    );
                    let rect = flea::ui_logic_pv1::rect::surface(
                        &Shape::unknown(State::Text),
                        width,
                        height,
                        false,
                        flea::ui_logic_pv1::rect::TOKENS,
                    );
                    let frame = (rect.w as f32, rect.h as f32 - crate::g2::pv1::look::BAR);
                    let view = cx.new(|cx| {
                        look.markdown(&path, None, &shell.locale.catalog, Some(frame), window, cx)
                    });
                    let watch = cx.observe(&view, |_, _, cx| cx.notify());
                    let warm = &mut shell.panes[side].as_mut().unwrap().look_warm;
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
        if !view.doc.complete || view.doc.status != crate::g2::pv2::markdown::Status::Ready {
            return None;
        }
        warm.prepared_watch = None;
        warm.prepared.take()
    }
}
