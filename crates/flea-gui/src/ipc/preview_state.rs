//! Oracle-shaped preview readers inspect the mounted views and their live model state.
#[path = "preview_quality.rs"]
mod quality;
use super::Adapter;
use crate::{
    columns::TrackRows,
    g2::{
        pv1::{doc::View as Document, QuickLook},
        pv2::PdfView,
    },
    state::Shell,
    window::{selection_preview::Kind, startup::View},
};
use flea::{ipc::wire::Reply, json::escape};
use gpui::{App, Window};

pub(crate) struct PreviewState<'a> {
    pub shell: &'a Shell,
    pub window: &'a Window,
    pub cx: &'a App,
}

impl PreviewState<'_> {
    fn look(&self) -> Option<&QuickLook> {
        self.shell
            .quick_look
            .as_ref()
            .map(|host| host.view.read(self.cx))
    }

    fn pdf(&self) -> Option<&PdfView> {
        let doc = self.look()?.subject()?.body.doc()?;
        match &doc.view {
            Document::Pdf(pdf) => Some(pdf.read(self.cx)),
            _ => None,
        }
    }

    fn selection(&self) -> Option<Reply> {
        let host = self.shell.panes[self.shell.focus_side].as_ref()?;
        let column = host.preview.as_ref().map(|view| view.read(self.cx));
        let visible = host.drawn_view == Some(View::Columns)
            && column.is_some_and(|view| view.visible)
            && host.columns.as_ref().is_some_and(|columns| {
                columns.preview_hosted
                    && columns
                        .area
                        .tracks()
                        .iter()
                        .any(|track| matches!(track.rows, TrackRows::Preview))
            });
        let frame = column.and_then(|view| view.pending.as_ref().or(view.drawn.as_ref()));
        let index = frame
            .filter(|_| visible)
            .map_or(-1, |frame| frame.subject.index as i64);
        let path = frame.map_or_else(String::new, |frame| {
            crate::navigation::history::join(&frame.subject.directory, &frame.subject.row.name)
        });
        let pending = frame
            .filter(|frame| !frame.manual && frame.kind != Kind::Multi && frame.meta.is_none())
            .map_or(0, |frame| frame.generation);
        let width = self.shell.geometry(self.window).panes[self.shell.focus_side].width;
        let view = match self.shell.view {
            View::List | View::Dual => "list",
            View::Grid => "grid",
            View::Columns => "columns",
        };
        let focused = column.is_some_and(|view| view.focus.is_focused(self.window));
        Some(Reply::value(format!(
            r#"{{"view":"{view}","width":{width},"available":{width},"inlineVisible":{visible},"index":{index},"path":"{}","focused":{focused},"pending":{pending}}}"#,
            escape(&path),
        )))
    }

    fn close(&self) -> Reply {
        let doc = self
            .look()
            .and_then(QuickLook::subject)
            .and_then(|subject| subject.body.doc());
        let Some((look, doc)) = self
            .look()
            .zip(doc)
            .filter(|(_, doc)| matches!(doc.view, Document::Markdown(_) | Document::Html(_)))
        else {
            return Reply::value("{}");
        };
        let observed = look.observations.borrow();
        let centre = observed.close.map_or_else(String::new, |bounds| {
            format!(
                "{} {}",
                f32::from(bounds.center().x).round(),
                f32::from(bounds.center().y).round()
            )
        });
        let hovered = observed.close_hovered || look.hovered() == Some(crate::g2::pv1::Hit::Close);
        let pressed = look.pressed() == Some(crate::g2::pv1::Hit::Close);
        let focused = look.focused() == Some(crate::g2::pv1::Hit::Close)
            || doc.chrome(&look.i18n, self.cx).close_focused;
        Reply::value(format!("{{\"hovered\":{hovered},\"pressed\":{pressed},\"focused\":{focused},\"centre\":\"{centre}\"}}"))
    }

    fn swap(&self) -> Reply {
        let column = self.shell.panes[self.shell.focus_side]
            .as_ref()
            .filter(|host| host.columns.is_some())
            .map_or_else(
                || "null".into(),
                |host| {
                    let frames = host
                        .preview
                        .as_ref()
                        .map(|view| view.read(self.cx).frames.borrow());
                    crate::window::selection_preview::observation::json(
                        &host.preview_port.0.borrow(),
                        frames.as_deref(),
                    )
                },
            );
        let (look, open, visible) = if let Some(look) = self.look() {
            let observations = look.observations.borrow();
            (
                observations.swap.json(),
                observations.open_json(look.is_revealed()),
                look.is_open(),
            )
        } else {
            (r#"{"holding":false,"capturing":false,"fellBack":false,"holds":0,"fallbacks":0,"bursts":0,"heldFrames":0,"midFrames":0,"loadingFrames":0,"last":{"ms":0,"end":""}}"#.into(),
             r#"{"revealed":false,"revealedEarly":false,"landed":false,"lookEmptyFrames":0,"revealCaps":0}"#.into(), false)
        };
        Reply::value(format!(
            "{{\"column\":{column},\"look\":{look},\"lookOpen\":{open},\"lookVisible\":{visible}}}"
        ))
    }
}

impl Adapter for PreviewState<'_> {
    fn call(&self, function: &str, _arguments: &[String]) -> Option<Reply> {
        let pdf = self.pdf();
        let value = match function {
            "previewPdfPage" => pdf.map_or(-1, |pdf| pdf.surface.page as i64).to_string(),
            "previewPdfFocus" => pdf
                .and_then(|pdf| pdf.viewer.as_ref())
                .and_then(|viewer| viewer.control)
                .map_or(-1, |control| control as i64)
                .to_string(),
            "previewPdfZoom" => pdf
                .and_then(|pdf| pdf.viewer.as_ref())
                .map_or_else(String::new, |viewer| viewer.zoom.to_string()),
            "previewExpanded" => pdf
                .and_then(|pdf| pdf.viewer.as_ref())
                .map_or_else(String::new, |viewer| viewer.expanded.to_string()),
            "previewSelectionState" => return self.selection(),
            "previewCloseState" => return Some(self.close()),
            "previewSwapState" => return Some(self.swap()),
            "previewQuality" => return Some(self.quality()),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
