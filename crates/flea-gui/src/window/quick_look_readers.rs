//! Quick Look's state, its mounted media clock and the Compare card, read through the state IPC.
use super::Host;
use crate::{
    g2::pv1::{doc::View, Body, Hit, QuickLook},
    ipc::Adapter,
};
use flea::{
    ipc::wire::Reply,
    json::escape,
    ui_logic_pv1::{compare::Kind, kinds::State},
};
use gpui::{App, Pixels, Size};

pub(crate) struct Reader<'a> {
    pub host: Option<&'a Host>,
    pub view: Option<&'a QuickLook>,
    pub viewport: Size<Pixels>,
    pub cx: &'a App,
    /// The mounted view's refusal or failure sentence, read where the App is at hand.
    pub failure: Option<String>,
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
            Some(Body::CannotShow { sentence, .. } | Body::Refused { sentence, .. }) => {
                sentence.clone()
            }
            Some(Body::Doc(_)) if self.failure.is_some() => {
                self.failure.clone().unwrap_or_default()
            }
            _ => self.kind().into(),
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
            subject.mark(),
            controls.join(","),
        )
    }
}

impl Adapter for Reader<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        Some(Reply::value(match function {
            "previewOpen" => self.open().is_some().to_string(),
            "previewKind" => self.kind().into(),
            "previewState" => self.state(),
            "previewPosition" => self.position().to_string(),
            "compareState" => self.compare(),
            _ => return None,
        }))
    }
}
