//! IpcPreviewState.htmlState: the HTML view Quick Look or the focused pane's column shows, as PreviewHtml.qml captureState writes it.
use super::preview_state::PreviewState;
use crate::{
    g2::{pv1::doc::View as Document, pv2::html::HtmlView},
    window::selection_preview::{docs::Doc, Mounted},
};
use flea::{ipc::wire::Reply, jsondoc::Json};
use gpui::Entity;

impl PreviewState<'_> {
    /// The open card's or the column's HTML view; anything else answers `{}` as the QML reader does.
    pub(super) fn html_state(&self, column: bool) -> Reply {
        let found = if column {
            self.column_html()
        } else {
            self.look_html()
        };
        let Some((view, visible)) = found else {
            return Reply::value("{}");
        };
        let html = view.read(self.cx);
        let doc = &html.doc;
        let catalog = &self.shell.locale.catalog;
        let lines = html.line_count(self.cx);
        let count = if column {
            catalog.trn("document.line_count", lines as i64)
        } else {
            html.bar(catalog, self.cx).count.unwrap_or_default()
        };
        let state = Json::Obj(vec![
            ("path".into(), Json::Str(doc.path.clone())),
            (
                "view".into(),
                Json::Str(
                    if doc.source_view() {
                        "source"
                    } else {
                        "rendered"
                    }
                    .into(),
                ),
            ),
            ("ready".into(), Json::Bool(html.first_screen(self.cx))),
            ("visible".into(), Json::Bool(visible)),
            ("state".into(), Json::Str(html.status(catalog, self.cx))),
            ("lines".into(), Json::Num(lines.to_string())),
            ("count".into(), Json::Str(count)),
            ("mark".into(), Json::Str("html".into())),
            ("file".into(), Json::Str(doc.image_file.clone())),
            ("width".into(), Json::Num(doc.rendered_width.to_string())),
            (
                "deviceWidth".into(),
                Json::Num(doc.device_width.to_string()),
            ),
            ("y".into(), Json::Num(html.scroll_y.to_string())),
            ("zoom".into(), Json::Num(doc.zoom.to_string())),
            ("column".into(), Json::Bool(column)),
        ]);
        Reply::value(flea::jsondoc::render_at(&state, 0))
    }

    fn look_html(&self) -> Option<(Entity<HtmlView>, bool)> {
        let look = self.shell.quick_look.as_ref()?.view.read(self.cx);
        let open = look.is_open();
        match &look.subject()?.body.doc()?.view {
            Document::Html(view) => Some((view.clone(), open)),
            _ => None,
        }
    }

    fn column_html(&self) -> Option<(Entity<HtmlView>, bool)> {
        let host = self.shell.panes[self.shell.focus_side].as_ref()?;
        let column = host.preview.as_ref()?.read(self.cx);
        match &column.drawn.as_ref()?.mounted {
            Mounted::Doc(Doc::Html(view)) => Some((view.clone(), column.visible)),
            _ => None,
        }
    }
}
