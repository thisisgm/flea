//! htmlState: the mounted Quick Look or column HTML view, as PreviewHtml.qml captureState reports it.
use super::{preview_state::PreviewState, Adapter};
use crate::{
    g2::{
        pv1::{doc::View, Hit},
        pv2::html::HtmlView,
    },
    ipc::rows::{centre_of, rect_of},
    state::Shell,
    visual::object,
    window::selection_preview::{docs::Doc, Mounted},
};
use flea::{ipc::wire::Reply, jsondoc::Json};
use gpui::Entity;
use gpui::{App, Bounds, Pixels, Window};

pub(crate) struct PreviewHtml<'a> {
    pub shell: &'a Shell,
    pub window: &'a Window,
    pub cx: &'a App,
}

fn text(value: impl Into<String>) -> Json {
    Json::Str(value.into())
}

fn number(value: impl ToString) -> Json {
    Json::Num(value.to_string())
}

fn rect(bounds: Option<Bounds<Pixels>>) -> String {
    bounds.map_or_else(String::new, |b| {
        rect_of(
            b.origin.x.into(),
            b.origin.y.into(),
            b.size.width.into(),
            b.size.height.into(),
        )
    })
}

impl PreviewHtml<'_> {
    // Quick Look's close control: its pointer and keyboard state and where it was laid out.
    fn close(&self) -> Json {
        let Some(host) = self.shell.quick_look.as_ref() else {
            return Json::Null;
        };
        let look = host.view.read(self.cx);
        let observed = look.observations.borrow();
        let centre = observed.close.map_or_else(String::new, |b| {
            centre_of(
                b.origin.x.into(),
                b.origin.y.into(),
                b.size.width.into(),
                b.size.height.into(),
            )
        });
        let focused = look.focused() == Some(Hit::Close);
        object([
            (
                "hovered",
                Json::Bool(observed.close_hovered || look.hovered() == Some(Hit::Close)),
            ),
            ("pressed", Json::Bool(look.pressed() == Some(Hit::Close))),
            ("focused", Json::Bool(focused)),
            ("ring", Json::Bool(focused)),
            ("centre", text(centre)),
        ])
    }

    fn state(&self, view: &HtmlView, visible: bool, facts: String, column: bool) -> String {
        let catalog = &self.shell.locale.catalog;
        let size = self.window.viewport_size();
        let state = object([
            ("path", text(view.doc.path.clone())),
            (
                "view",
                text(if view.doc.source_view() {
                    "source"
                } else {
                    "rendered"
                }),
            ),
            ("ready", Json::Bool(view.first_screen(self.cx))),
            ("visible", Json::Bool(visible)),
            ("state", text(view.status(catalog, self.cx))),
            ("lines", number(view.line_count(self.cx))),
            (
                "count",
                text(if column {
                    catalog.trn("document.line_count", view.line_count(self.cx) as i64)
                } else {
                    view.bar(catalog, self.cx).count.unwrap_or_default()
                }),
            ),
            ("mark", text("html")),
            ("file", text(view.doc.image_file.clone())),
            ("width", number(view.doc.rendered_width)),
            ("deviceWidth", number(view.doc.device_width)),
            ("y", number(view.scroll_y)),
            ("zoom", number(view.doc.zoom)),
            ("facts", text(facts)),
            ("frame", text(rect(view.bounds))),
            ("column", Json::Bool(column)),
            (
                "window",
                object([
                    ("width", number(f32::from(size.width))),
                    ("height", number(f32::from(size.height))),
                ]),
            ),
            ("radius", number(view.metrics.radius)),
            ("viewportHeight", number(view.frame.1)),
            ("contentHeight", number(view.page_size().1)),
            ("close", if column { Json::Null } else { self.close() }),
        ]);
        flea::jsondoc::render(&state).trim_end().into()
    }

    fn column(&self) -> Option<String> {
        let entity = self.shell.selection_preview(self.shell.focus_side)?;
        let preview = entity.read(self.cx);
        let frame = preview.drawn.as_ref()?;
        let Mounted::Doc(Doc::Html(view)) = &frame.mounted else {
            return None;
        };
        let (kind, tail) = preview.compact(frame, self.cx);
        Some(self.state(view.read(self.cx), preview.visible, kind + &tail, true))
    }

    fn look(&self) -> Option<String> {
        let look = self.shell.quick_look.as_ref()?.view.read(self.cx);
        if !look.is_open() {
            return None;
        }
        let View::Html(view) = &look.subject()?.body.doc()?.view else {
            return None;
        };
        Some(self.state(view.read(self.cx), look.is_revealed(), String::new(), false))
    }
}

impl Adapter for PreviewHtml<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        if function != "htmlState" {
            return None;
        }
        let value = if arguments.first().is_some_and(|flag| flag == "true") {
            self.column()
        } else {
            self.look()
        };
        Some(value.map_or_else(
            || {
                PreviewState {
                    shell: self.shell,
                    window: self.window,
                    cx: self.cx,
                }
                .html_state(arguments.first().is_some_and(|flag| flag == "true"))
            },
            Reply::value,
        ))
    }
}

impl PreviewState<'_> {
    /// The open card's or the column's HTML view; anything else answers `{}` as the QML reader does.
    pub(super) fn html_state(&self, column: bool) -> Reply {
        let found = if column {
            self.column_html()
        } else {
            self.look_html()
        };
        let Some((view, visible, facts)) = found else {
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
            ("facts".into(), Json::Str(facts)),
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

    fn look_html(&self) -> Option<(Entity<HtmlView>, bool, String)> {
        let look = self.shell.quick_look.as_ref()?.view.read(self.cx);
        let open = look.is_open();
        match &look.subject()?.body.doc()?.view {
            View::Html(view) => Some((view.clone(), open, String::new())),
            _ => None,
        }
    }

    fn column_html(&self) -> Option<(Entity<HtmlView>, bool, String)> {
        let host = self.shell.panes[self.shell.focus_side].as_ref()?;
        let column = host.preview.as_ref()?.read(self.cx);
        let frame = column.drawn.as_ref()?;
        match &frame.mounted {
            Mounted::Doc(Doc::Html(view)) => {
                let (kind, tail) = column.compact(frame, self.cx);
                Some((view.clone(), column.visible, kind + &tail))
            }
            _ => None,
        }
    }
}
