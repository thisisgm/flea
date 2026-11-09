//! htmlState: the mounted Quick Look or column HTML view, as PreviewHtml.qml captureState reports it.
use super::Adapter;
use crate::{
    g2::{
        doc_preview::html::HtmlView,
        quick_look_overlay::{doc::View, Hit},
    },
    ipc::rows::{centre_of, colour, rect_of},
    state::Shell,
    visual::object,
    window::selection_preview::{docs::Doc, Mounted},
};
use flea::{ipc::wire::Reply, jsondoc::Json};
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
        let style = view.frame_style.get();
        let (y, viewport_height, content_height) = view.viewport_metrics(self.cx);
        let lane = view.vertical_lane(self.cx).map_or(Json::Null, |lane| {
            object([
                ("shown", Json::Bool(lane.shown)),
                ("visible", Json::Bool(visible && lane.visible)),
                ("rect", text(rect(Some(lane.bounds)))),
            ])
        });
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
            ("y", number(y)),
            ("zoom", number(view.doc.zoom)),
            ("facts", text(facts)),
            ("frame", text(rect(view.frame_bounds.get()))),
            ("column", Json::Bool(column)),
            (
                "window",
                object([
                    ("width", number(f32::from(size.width))),
                    ("height", number(f32::from(size.height))),
                ]),
            ),
            ("radius", number(view.metrics.radius)),
            ("hairline", style.map_or(Json::Null, |s| number(s.hairline))),
            (
                "ringColor",
                style.map_or(Json::Null, |s| text(colour(s.ring))),
            ),
            (
                "canvasColor",
                style.map_or(Json::Null, |s| text(colour(s.canvas))),
            ),
            ("lane", lane),
            ("viewportHeight", number(viewport_height)),
            ("contentHeight", number(content_height)),
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
        // Anything but a mounted HTML view answers `{}` as the QML reader does.
        Some(Reply::value(value.as_deref().unwrap_or("{}")))
    }
}
