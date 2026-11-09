//! A matching mounted column image supplies first paint, then the card decodes its original.
use super::*;
use crate::window::selection_preview::{Kind, Mounted};
use std::{path::PathBuf, sync::Arc};

impl Shell {
    fn column_picture(
        &self,
        side: usize,
        index: usize,
        row: &RowFacts,
        path: &str,
        cx: &gpui::App,
    ) -> Option<(
        flea::ui_logic_preview_core::meta::Meta,
        Arc<gpui::RenderImage>,
    )> {
        let host = self.panes.get(side)?.as_ref()?;
        let preview = host.preview.as_ref()?.read(cx);
        let frame = preview.drawn.as_ref()?;
        if !preview.visible
            || preview.pending.is_some()
            || frame.kind != Kind::Image
            || !matches!(frame.mounted, Mounted::Basic)
            || frame.failed
            || frame.source.is_none()
            || frame.subject.directory != host.path
            || frame.subject.listing != host.pane.held.listing
            || frame.subject.index != index
            || frame.subject.row != *row
            || crate::navigation::history::join(&frame.subject.directory, &row.name) != path
            || row.link.is_some()
        {
            return None;
        }
        let meta = frame.facts.as_ref()?;
        if meta.w == 0 || meta.h == 0 || meta.raw.is_some() {
            return None;
        }
        Some((meta.clone(), frame.image.clone()?))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn show_picture(
        &mut self,
        side: usize,
        index: usize,
        row: &RowFacts,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let interim = self.column_picture(side, index, row, &path, cx);
        let (body, shape) = interim.as_ref().map_or_else(
            || {
                (
                    Body::Image {
                        path: path.clone().into(),
                        w: 0.,
                        h: 0.,
                    },
                    Shape::unknown(State::Image),
                )
            },
            |(meta, _)| {
                (
                    keys::picture(meta, path.clone(), None, &self.locale.catalog),
                    Shape {
                        kind: State::Image,
                        known: true,
                        w: meta.w.into(),
                        h: meta.h.into(),
                        orient: meta.orient.max(1),
                    },
                )
            },
        );
        let subject = Subject {
            name: row.name.clone(),
            icon: row.icon.clone(),
            mode: row.mode,
            state: State::Image,
            shape,
            body,
        };
        let token = token();
        let line = format!(
            r#"{{"c":"meta","row":{index},"token":{token},"text":false,"media":false,"archive":false}}"#
        );
        let ask = Ask::Meta {
            path: path.clone(),
            link: row.link.clone(),
        };
        // Without a column frame the card reads the file's own header while meta is out (EY-521).
        let early = replies::header_path(&ask, &subject.body).filter(|_| interim.is_none());
        let host = self.quick_look_host(side, window, cx);
        host.token = token;
        host.row = Some(index);
        host.ask = ask;
        self.show(subject, false, Some(line), window, cx);
        self.quick_look_header(token, early, cx);
        if let (Some((_, image)), Some(host)) = (interim, self.quick_look.as_ref()) {
            host.view.update(cx, |view, cx| {
                view.picture_interim(PathBuf::from(path), image, cx);
            });
        }
    }
}
