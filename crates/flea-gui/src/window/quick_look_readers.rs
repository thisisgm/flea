//! Quick Look's state, its mounted media clock and the Compare card, read through the state IPC.
use super::Host;
use crate::{
    g2::{
        doc_preview::{markdown::MarkdownView, markdown::View as Shown, PdfView},
        media_preview::{sound::MediaSound, strip::StripCache},
        quick_look_overlay::{
            doc::{Doc, View},
            Body, Hit, QuickLook,
        },
    },
    ipc::Adapter,
};
use flea::{
    ipc::wire::Reply,
    json::escape,
    ui_logic_preview_core::{compare::Kind, kinds::State},
};
use gpui::{App, Bounds, Pixels, Size};

#[path = "quick_look_reader_geometry.rs"]
mod geometry;

pub(crate) struct Reader<'a> {
    pub host: Option<&'a Host>,
    pub view: Option<&'a QuickLook>,
    pub viewport: Size<Pixels>,
    pub cx: &'a App,
    /// The mounted view's refusal or failure sentence, read where the App is at hand.
    pub failure: Option<String>,
    /// Whether the open PDF holds the keyboard, which pdfState(true) answers as focused.
    pub pdf_focused: bool,
}

impl Reader<'_> {
    fn open(&self) -> Option<&QuickLook> {
        self.view.filter(|view| view.is_open())
    }

    /// Milliseconds from the card's own player, with the column clock's -1 sentinel when no media is mounted.
    fn position(&self) -> i64 {
        let Some(doc) = self
            .open()
            .and_then(|view| view.subject())
            .and_then(|subject| subject.body.doc())
        else {
            return -1;
        };
        match &doc.view {
            View::Media(media) => media.player.read(self.cx).position,
            _ => -1,
        }
    }

    /// Preview.qml kind: the open card's kind name, compare for a pair, "" with no card.
    fn kind(&self) -> &'static str {
        let Some(subject) = self.open().and_then(QuickLook::subject) else {
            return "";
        };
        if subject.body.is_compare() {
            return "compare";
        }
        if let Some(doc) = subject.body.doc() {
            return doc.kind();
        }
        match subject.state {
            State::Image | State::Symlink => "image",
            State::Video => "video",
            State::Audio => "audio",
            State::Pdf => "pdf",
            State::Text | State::Code => "text",
            State::Archive => "archive",
            State::Font => "font",
            _ => "unsupported",
        }
    }

    /// Preview.qml status: loading until the swap reveals the card, then the refusal sentence or the kind.
    fn state(&self) -> String {
        let Some(view) = self.open() else {
            return String::new();
        };
        if !view.is_revealed() {
            return "loading".into();
        }
        match view.subject().map(|s| &s.body) {
            // CompareView.qml:33 keeps Preview's status at loading until the diff answers.
            Some(Body::Refused { sentence, .. }) if sentence.is_empty() => "loading".into(),
            Some(Body::CannotShow { sentence, .. } | Body::Refused { sentence, .. }) => {
                sentence.clone()
            }
            Some(Body::Doc(_)) if self.failure.is_some() => {
                self.failure.clone().unwrap_or_default()
            }
            Some(Body::Doc(doc)) => self.doc_state(doc),
            _ => self.kind().into(),
        }
    }

    /// Each Preview.qml pane's own status: the player's word for media, ready once a text kind's lines landed (EY-447).
    fn doc_state(&self, doc: &Doc) -> String {
        let ready = |landed: bool| if landed { "ready" } else { "loading" };
        match &doc.view {
            View::Media(media) => media.player.read(self.cx).status().word().into(),
            View::Code(code) => ready(code.read(self.cx).lines.answer.settled()).into(),
            View::Markdown(view) => ready(view.read(self.cx).doc.content_ready()).into(),
            View::Html(_) | View::Table(_) => "ready".into(),
            _ => doc.kind().into(),
        }
    }

    /// IpcPreviewState previewText: the open card's text as shown, or None so the column answers.
    fn text(&self) -> Option<String> {
        match self.view()? {
            View::Code(code) => Some(code.read(self.cx).lines.plain_lines().join("\n")),
            _ => None,
        }
    }

    /// IpcPreviewState compareState: the open pair's answer, bar, rows and controls; opened false with none.
    fn compare(&self) -> String {
        let Some((view, subject)) = self
            .open()
            .and_then(|view| Some((view, view.subject()?)))
            .filter(|(_, s)| s.body.is_compare())
        else {
            return r#"{"opened":false}"#.into();
        };
        let host = self.host;
        let answer = host.map_or("", |h| h.answer.as_str());
        let patch = host.map_or("", |h| h.patch.as_str());
        let (title, sentence, rows, copy) = match &subject.body {
            Body::Diff { title, copy, rows } => {
                (title.as_str(), "", rows.as_slice(), Some(copy.as_str()))
            }
            Body::Refused { title, sentence } => (title.as_str(), sentence.as_str(), &[][..], None),
            _ => unreachable!("filtered to a compare body"),
        };
        let rows: Vec<String> = rows
            .iter()
            .map(|row| {
                let kind = match row.kind {
                    Kind::Hunk => "hunk",
                    Kind::Context => "context",
                    Kind::Removed => "removed",
                    Kind::Added => "added",
                    Kind::Note => "note",
                };
                format!(
                    r#"{{"kind":"{kind}","oldNo":{},"newNo":{},"text":"{}"}}"#,
                    row.old_no,
                    row.new_no,
                    escape(&row.text)
                )
            })
            .collect();
        let card = view.card(self.viewport);
        let rect = format!(
            "{} {} {} {}",
            f32::from(card.origin.x),
            f32::from(card.origin.y),
            f32::from(card.size.width),
            f32::from(card.size.height)
        );
        let control = |name: &str, label: &str, hit: Hit, shown: bool| {
            format!(
                r#"{{"name":"{name}","label":"{}","visible":{shown},"enabled":{shown},"focused":{},"hovered":{},"pressed":{}}}"#,
                escape(label),
                view.focused() == Some(hit),
                view.hovered() == Some(hit),
                view.pressed() == Some(hit),
            )
        };
        let close = view.i18n.tr("preview.close");
        let controls = [
            control("copy", copy.unwrap_or(""), Hit::CopyPatch, copy.is_some()),
            control("close", &close, Hit::Close, true),
        ];
        format!(
            r#"{{"opened":true,"visible":true,"landed":{},"ready":{},"answer":"{}","title":"{}","sentence":"{}","patch":"{}","rows":[{}],"mark":"{:?}","rect":"{rect}","controls":[{}]}}"#,
            view.is_revealed(),
            !answer.is_empty(),
            escape(answer),
            escape(title),
            escape(sentence),
            escape(patch),
            rows.join(","),
            view.header_mark().expect("an open subject"),
            controls.join(","),
        )
    }
}

impl Reader<'_> {
    /// The open card's mounted document; None while Quick Look is shut or shows a refusal or a comparison.
    fn doc(&self) -> Option<&Doc> {
        self.open()
            .and_then(QuickLook::subject)
            .and_then(|subject| subject.body.doc())
    }

    fn view(&self) -> Option<&View> {
        self.doc().map(|doc| &doc.view)
    }

    fn markdown(&self) -> Option<&MarkdownView> {
        match self.view() {
            Some(View::Markdown(view)) => Some(view.read(self.cx)),
            _ => None,
        }
    }

    fn pdf(&self) -> Option<&PdfView> {
        match self.view() {
            Some(View::Pdf(pdf)) => Some(pdf.read(self.cx)),
            _ => None,
        }
    }

    /// IpcPreviewState previewDuration: the mounted player's length in milliseconds, 0 with no media.
    fn duration(&self) -> i64 {
        match self.view() {
            Some(View::Media(media)) => media.player.read(self.cx).duration,
            _ => 0,
        }
    }

    /// Rendered Markdown's end gap, clamped to 0; -1 until ready or while Source is shown.
    fn end_gap(&self) -> i64 {
        match self.markdown() {
            Some(view) if view.shown_view() == Shown::Rendered && view.doc.content_ready() => {
                (view.content_height() - view.scroll - view.frame.1)
                    .round()
                    .max(0.) as i64
            }
            _ => -1,
        }
    }

    /// The media strip's box from its last paint as "x y" centre; "" until a Quick Look media card has painted it.
    fn strip_centre(&self, part: impl Fn(&StripCache) -> Option<Bounds<Pixels>>) -> String {
        match self.view() {
            Some(View::Media(media)) => {
                part(&media.caches.strip.borrow()).map_or_else(String::new, |bounds| {
                    let centre = bounds.center();
                    format!(
                        "{} {}",
                        f32::from(centre.x).round(),
                        f32::from(centre.y).round()
                    )
                })
            }
            _ => String::new(),
        }
    }

    /// A mounted media strip's painted visibility, mute state and centre; other kinds remain unported.
    fn strip(&self) -> Option<String> {
        match self.view() {
            Some(View::Media(media)) => Some(format!(
                r#"{{"visible":{},"muted":{},"mute":"{}"}}"#,
                media.caches.strip.borrow().mute.is_some(),
                MediaSound::get(self.cx).muted,
                self.strip_centre(|cache| cache.mute),
            )),
            _ => None,
        }
    }

    /// IpcPreviewState archiveNames: the index names joined by bars, "" until the index lands.
    fn archive_names(&self) -> String {
        match self.view() {
            Some(View::Archive(archive)) => {
                archive
                    .read(self.cx)
                    .meta
                    .as_ref()
                    .map_or_else(String::new, |meta| {
                        meta.names
                            .iter()
                            .map(|(name, _)| name.as_str())
                            .collect::<Vec<_>>()
                            .join("|")
                    })
            }
            _ => String::new(),
        }
    }
}

impl Adapter for Reader<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        Some(Reply::value(match function {
            "previewOpen" => self.open().is_some().to_string(),
            "previewKind" => self.kind().into(),
            "previewState" => self.state(),
            "previewPosition" => self.position().to_string(),
            "previewDuration" => self.duration().to_string(),
            // IpcPreviewState previewMediaLoaded reads the card's player while it is open; a shut card leaves the column's.
            "previewMediaLoaded" if self.open().is_some() => {
                matches!(self.view(), Some(View::Media(_))).to_string()
            }
            "previewEndGap" => self.end_gap().to_string(),
            "previewMarkdownView" => self
                .markdown()
                .map_or_else(String::new, |view| view.shown_view().name().into()),
            // Only a Markdown document has a text the reader can count; other kinds stay unported.
            "previewTextLength" => match self.doc() {
                None => "0".into(),
                Some(doc) => match &doc.view {
                    View::Markdown(view) => view
                        .read(self.cx)
                        .doc
                        .text
                        .as_deref()
                        .map_or(0, |text| text.encode_utf16().count())
                        .to_string(),
                    _ => return None,
                },
            },
            "previewExpanded" => self
                .pdf()
                .and_then(|pdf| pdf.viewer.as_ref())
                .map_or_else(String::new, |viewer| viewer.expanded.to_string()),
            "previewPdfPage" => self
                .pdf()
                .map_or(-1, |pdf| pdf.surface.page as i64)
                .to_string(),
            "previewPdfZoom" => self
                .pdf()
                .and_then(|pdf| pdf.viewer.as_ref())
                .map_or_else(String::new, |viewer| viewer.zoom.to_string()),
            "previewPdfFocus" => self
                .pdf()
                .and_then(|pdf| pdf.viewer.as_ref())
                .and_then(|viewer| viewer.control)
                .map_or(-1, |index| index as i64)
                .to_string(),
            "previewText" => self.text()?,
            "previewArchiveNames" => self.archive_names(),
            "previewSliderCentre" => self.strip_centre(|cache| cache.seek),
            "previewStrip" => self.strip()?,
            "previewSurfaceRect" => self.surface_rect(false),
            "previewPictureRect" => self.surface_rect(true),
            "pdfState" if arguments.first().is_some_and(|overlay| overlay == "true") => {
                self.pdf_state()
            }
            "compareState" => self.compare(),
            _ => return None,
        }))
    }
}
