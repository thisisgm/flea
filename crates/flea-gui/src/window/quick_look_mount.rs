//! Which view a row mounts in the card, how the Shell mounts it, sends its asks and hands it the backend's answers.
use super::{
    docs::{Look, Mounting},
    wire::{code_reply, font_reply, html_reply, markdown_answer, SLOT},
};
use crate::{
    backend::Event as BackendEvent,
    g2::doc_preview::code::Mark,
    g2::quick_look_overlay::{
        doc::{Doc, View},
        glyph::Glyph,
        Body, Subject,
    },
    state::Shell,
};
use flea::{
    jsondoc::Json,
    protocol::RowFacts,
    ui_logic_preview_core::kinds::{self, State},
};
use gpui::{App, Context, Window};

#[path = "quick_look_plugin.rs"]
mod plugin;

/// Which view a row mounts in the card; None leaves it to the card's own bodies (image, symlink, compare, cannot preview).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mount {
    Text,
    Code,
    Markdown,
    Html,
    Table,
    Office,
    Pdf,
    Font,
    Archive,
    Model,
    Video,
    Audio,
}

impl Mount {
    pub(crate) fn of(row: &RowFacts) -> Option<Self> {
        let name = row.name.as_str();
        let ext = name
            .rsplit_once('.')
            .map_or("", |(_, e)| e)
            .to_ascii_lowercase();
        // The column's model list (selection_preview.rs Kind::of), so G-code and PLY open the same viewer there and here.
        if matches!(
            ext.as_str(),
            "stl" | "3mf" | "obj" | "ply" | "amf" | "gcode" | "gco" | "bgcode"
        ) {
            return Some(Mount::Model);
        }
        if flea::ui_logic_doc_preview::office::Group::of(name).is_some() {
            return Some(Mount::Office);
        }
        if kinds::is_markdown(name) {
            return Some(Mount::Markdown);
        }
        if kinds::is_html(name) {
            return Some(Mount::Html);
        }
        if matches!(ext.as_str(), "csv" | "tsv") {
            return Some(Mount::Table);
        }
        Some(match kinds::quick_look_kind(&row.icon, name, false) {
            State::Pdf => Mount::Pdf,
            State::Font => Mount::Font,
            State::Archive => Mount::Archive,
            State::Video => Mount::Video,
            State::Audio => Mount::Audio,
            State::Text if kinds::is_code(name) => Mount::Code,
            State::Text | State::Code => Mount::Text,
            _ => return None,
        })
    }

    /// The card state the subject is sized and named by while its view loads.
    pub(crate) fn state(self) -> State {
        match self {
            Mount::Code | Mount::Html => State::Code,
            Mount::Pdf | Mount::Office => State::Pdf,
            Mount::Font => State::Font,
            Mount::Archive => State::Archive,
            Mount::Video => State::Video,
            Mount::Audio => State::Audio,
            Mount::Model => State::Image,
            Mount::Text | Mount::Markdown | Mount::Table => State::Text,
        }
    }

    /// The meta ask this kind's view reads: the media probe and an archive's own index.
    pub(crate) fn meta(self) -> Option<(bool, bool)> {
        match self {
            Mount::Video | Mount::Audio => Some((true, false)),
            Mount::Archive => Some((false, true)),
            _ => None,
        }
    }
}

impl Shell {
    /// A terminal pane event fails pending code reads and preserves documents with no outstanding read.
    pub(super) fn quick_look_code_lost(&mut self, side: usize, cx: &mut Context<Self>) {
        let code = self
            .quick_look
            .as_ref()
            .filter(|host| host.side == side)
            .and_then(|host| {
                let doc = host.view.read(cx).subject()?.body.doc()?;
                match &doc.view {
                    View::Code(code) if code.read(cx).lines.token != 0 => Some(code.clone()),
                    _ => None,
                }
            });
        if let Some(code) = code {
            code.update(cx, |code, cx| code.set_read_failed(cx));
        }
    }

    /// Mounts the row's view in the card; false when it has none to play through, so the card's own refusal shows.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn show_doc(
        &mut self,
        side: usize,
        index: usize,
        row: &RowFacts,
        path: &str,
        kind: Mount,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let look = Look {
            theme,
            face: self.active_typeface(),
            muted: crate::list::Palette::inherited(&window.text_style(), theme).caption(false),
            mark: mark(row),
        };
        let catalog = self.locale.catalog.clone();
        let prepared = self.take_quick_prepared(side, row, path, cx);
        let was_prepared = prepared.is_some();
        let host = self.quick_look_host(side, window, cx);
        let mounting = Mounting {
            path,
            row,
            look: &look,
            wire: &host.wire.clone(),
            catalog: &catalog,
            backend: host.backend.clone(),
        };
        let Some(mut doc) = prepared.or_else(|| mounting.mount(kind, window, cx)) else {
            return false;
        };
        if let View::Markdown(view) = &doc.view {
            let shown = host.markdown_view;
            view.update(cx, |view, cx| view.set_view(shown, cx));
        }
        if let View::Code(code) = &doc.view {
            if let Some(hit) = self.panes[side]
                .as_ref()
                .and_then(|host| host.search.owner.match_line(host.pane.held.listing, index))
            {
                code.update(cx, |code, cx| {
                    code.reveal_match(hit.number, &hit.text, hit.run, theme.accent, cx)
                });
            }
        }
        super::docs::wrap_saved(&doc.view, &self.launch.settings.data, cx);
        self.lend_quick_prepared(side, index, row, &doc, cx);
        if was_prepared {
            if let crate::g2::quick_look_overlay::doc::View::Markdown(view) = &doc.view {
                let wire = self.quick_look.as_ref().unwrap().wire.clone();
                view.update(cx, |view, cx| {
                    view.rewind(cx);
                    view.set_code_host(Box::new(wire), cx);
                    view.finish_prepare(cx);
                });
            }
        }
        // The column's PDF hands the card the page it shows, so Quick Look opens there.
        if kind == Mount::Pdf {
            let column = self.selection_preview(side);
            let pdf =
                column.and_then(|column| super::super::preview_input::column_pdf(&column, cx));
            doc.page = pdf.map_or(0, |pdf| pdf.read(cx).surface.page);
        }
        let state = kind.state();
        let subject = Subject {
            name: row.name.clone(),
            icon: row.icon.clone(),
            mode: row.mode,
            state,
            shape: doc.shape(state, cx),
            body: Body::Doc(doc.clone()),
        };
        let token = super::token();
        let host = self.quick_look.as_mut().expect("built above");
        host.token = 0;
        host.row = Some(index);
        host.ask = super::Ask::Nothing;
        host.meta = 0;
        // Preview.qml: the media probe and an archive's own index ride the meta ask.
        let line = kind.meta().map(|(media, archive)| {
            host.meta = token;
            format!(
                r#"{{"c":"meta","row":{index},"token":{token},"text":false,"media":{media},"archive":{archive}}}"#
            )
        });
        let revealed = doc.ready(cx);
        self.show(subject, revealed, line, window, cx);
        self.quick_look_drain();
        true
    }

    /// The mounted view's failure sentence, for previewState.
    pub(crate) fn quick_look_failure(&self, cx: &App) -> Option<String> {
        let host = self.quick_look.as_ref()?;
        let view = host.view.read(cx);
        let doc = view.subject()?.body.doc()?;
        if let View::Code(code) = &doc.view {
            use flea::ui_logic_doc_preview::refusal::Refusal;
            let key = match code.read(cx).refusal() {
                Some(Refusal::TooLarge) => Some("preview.too_large"),
                Some(Refusal::Binary) => Some("preview.unsupported"),
                _ => None,
            };
            if let Some(key) = key {
                return Some(view.i18n.tr(key));
            }
        }
        doc.failure(&view.i18n, cx).map(|failure| failure.sentence)
    }

    /// Sends what the mounted view asked for since the last drain, through the card's pane.
    pub(crate) fn quick_look_drain(&mut self) {
        let Some(host) = self.quick_look.as_ref() else {
            return;
        };
        let (side, lines) = (host.side, host.wire.take());
        for line in lines {
            self.quick_look_send(side, line);
        }
    }

    /// A backend line for the mounted view or its meta ask; false for every other line.
    pub(super) fn quick_look_doc_line(
        &mut self,
        side: usize,
        event: &BackendEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let BackendEvent::Producer(line) = event else {
            return false;
        };
        let Some(host) = self.quick_look.as_ref().filter(|h| h.side == side) else {
            return false;
        };
        let view = host.view.clone();
        let meta = host.meta;
        let Some(doc) = view.read(cx).subject().and_then(|s| s.body.doc()).cloned() else {
            return false;
        };
        let Ok(json) = flea::jsondoc::parse(line) else {
            return false;
        };
        let taken = if meta != 0
            && json.get("t").and_then(Json::as_str) == Some("meta")
            && json.get("token").and_then(Json::as_f64) == Some(meta as f64)
        {
            self.quick_look.as_mut().expect("read above").meta = 0;
            view.update(cx, |ql, cx| {
                let doc = ql.subject.as_mut().and_then(|s| match &mut s.body {
                    Body::Doc(doc) => Some(doc),
                    _ => None,
                });
                let taken = doc.is_some_and(|doc| on_meta(doc, line, cx));
                cx.notify();
                taken
            })
        } else {
            on_line(&doc, &json, line, window, cx)
        };
        self.quick_look_drain();
        taken
    }
}

/// The row's own mark, which every view's refusal and chrome draw.
pub(crate) fn mark(row: &RowFacts) -> Mark {
    Glyph::kind(&row.icon, row.mode).svg()
}

/// One backend line for the mounted view; true when it took it.
pub(crate) fn on_line(
    doc: &Doc,
    json: &Json,
    line: &str,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let kind = json.get("t").and_then(Json::as_str).unwrap_or("");
    let slot = json.get("slot").and_then(Json::as_str).unwrap_or("");
    let mine = slot == SLOT || slot.starts_with("look:");
    match (&doc.view, kind) {
        (_, "code" | "html") if !mine => false,
        (View::Code(view), "code") => {
            let reply = code_reply(json);
            let taken = view.read(cx).lines.token == reply.token;
            view.update(cx, |view, cx| view.reply(reply, cx));
            taken
        }
        (View::Markdown(view), "code") => {
            view.update(cx, |view, cx| view.code_answered(markdown_answer(json), cx));
            true
        }
        (View::Table(view), "code") => view.update(cx, |view, cx| view.answered(line, cx)),
        (View::Html(view), "code") => {
            let Some(source) = view.read(cx).source.clone() else {
                return false;
            };
            source.update(cx, |code, cx| code.reply(code_reply(json), cx));
            true
        }
        (View::Html(view), "html") => {
            view.update(cx, |view, cx| view.reply(html_reply(json), cx));
            true
        }
        (View::Office(view), "code") => view
            .read(cx)
            .table
            .clone()
            .update(cx, |table, cx| table.answered(line, cx)),
        (View::Office(view), _) => view.update(cx, |view, cx| view.reply(line, window, cx)),
        (View::Font(view), "fontlooked") => {
            let reply = font_reply(json);
            let pending = view.read(cx).page.pending;
            if pending == 0 || reply.id != pending {
                return false;
            }
            view.update(cx, |view, cx| view.answered(reply, cx));
            true
        }
        (View::Model(view), "model3d") => view.update(cx, |view, cx| view.on_line(line, cx)),
        (View::Pdf(view), "pdfcopied") => {
            let text = |key| json.get(key).and_then(Json::as_str).unwrap_or("");
            let id = json.get("id").and_then(Json::as_f64).unwrap_or(0.) as u64;
            if id == 0
                || view.read(cx).surface.fetch
                    != flea::ui_logic_doc_preview::pdf::Fetch::Waiting(id)
            {
                return false;
            }
            let stalled = json.get("stalled").and_then(Json::as_bool) == Some(true);
            view.update(cx, |pdf, cx| {
                pdf.copied(id, text("path"), text("err"), stalled, cx)
            });
            true
        }
        _ => false,
    }
}

/// The card's meta answer for a mounted view: an archive's index, or what the media probe found.
pub(crate) fn on_meta(doc: &mut Doc, line: &str, cx: &mut App) -> bool {
    let Some(meta) = flea::ui_logic_preview_core::meta::parse(line) else {
        return false;
    };
    match &mut doc.view {
        View::Archive(view) => {
            let names = meta.names.into_iter().map(|e| (e.name, e.is_dir)).collect();
            let archive = flea::ui_logic_doc_preview::archive::Meta {
                entries: meta.entries.unwrap_or(0),
                unpacked: meta.unpacked,
                failed: meta.archive_failed,
                names,
            };
            let path = doc.path.clone();
            view.update(cx, |view, cx| view.answered(&path, archive, cx))
        }
        View::Media(media) => {
            media.known.rate_hz = meta.sample_rate.round() as i64;
            media.known.probe_duration_ms = meta.duration_ms;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "quick_look_mount_tests.rs"]
mod tests;
