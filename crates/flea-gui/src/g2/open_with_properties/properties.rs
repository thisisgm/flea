//! The Properties card: one item's two-column facts with a folder walk counted off the UI thread, or several items' summary.
mod readers;

use super::{
    keys::Key,
    shell::{Block, Button, Card, Fact, Ink},
    Effect,
};
use flea::i18n::I18n;
use flea::jsondoc::Json;
use flea::ui_logic_open_with_properties::openwith::close_request;
use flea::ui_logic_open_with_properties::properties::{
    self as logic, Facts, Label, Name, Several, Walk,
};

#[cfg(test)]
#[path = "m9_g2rows_05_tests.rs"]
mod tests;

/// The board's card width at its 14 body.
pub const WIDTH: f32 = 552.;

#[derive(Clone, Debug, PartialEq)]
enum Shown {
    Asking,
    One(Facts, Option<Walk>),
    Several(Several),
    Refused(String),
}

#[derive(Clone, Debug)]
pub struct Properties {
    id: u64,
    count: usize,
    closed: bool,
    /// Set once the Properties request is out, so a repeated snapshot answer cannot ask twice.
    asked: bool,
    shown: Shown,
    /// Pairs a host sets ahead of the media pairs (the window sets none) and the MIME database's kind word the host holds.
    extras: Vec<(String, String)>,
    mime_kind: String,
    /// The facts reply's codecs, bit rate and deferred EXIF for one file, None until it lands.
    media: Option<flea::ui_logic_preview_core::property::Facts>,
}

impl Properties {
    fn new(id: u64, count: usize) -> Self {
        Self {
            id,
            count,
            closed: false,
            asked: false,
            shown: Shown::Asking,
            extras: Vec::new(),
            mime_kind: String::new(),
            media: None,
        }
    }

    /// Sends the snapshot alone; Properties follows its acknowledgement, since the backend's one menu slot refuses a second queued request.
    pub fn open(id: u64, listing: usize, rows: &[usize], cursor: usize) -> (Self, Vec<Effect>) {
        (
            Self::new(id, rows.len()),
            vec![Effect::Send(logic::snapshot_request(
                id, listing, rows, cursor,
            ))],
        )
    }

    /// Reuse the menu's acknowledged snapshot; the host retains its side and sends effects through that backend.
    pub fn from_captured(
        captured: &crate::g2::context_menus::join::Captured,
    ) -> Option<(Self, Vec<Effect>)> {
        if !captured.ready || captured.id == 0 || captured.rows.is_empty() {
            return None;
        }
        Some((
            Self {
                asked: true,
                ..Self::new(captured.id, captured.rows.len())
            },
            vec![Effect::Send(logic::properties_request(captured.id))],
        ))
    }

    /// A complete card snapshot the host can prepare before its atomic presentation swap.
    pub fn ready(&self) -> bool {
        !self.closed && !matches!(self.shown, Shown::Asking)
    }

    pub fn set_extras(&mut self, extras: Vec<(String, String)>, mime_kind: &str) {
        if self.closed {
            return;
        }
        self.extras = extras;
        self.mime_kind = mime_kind.to_string();
    }

    /// Closing cancels the old id, so a late reply for it cannot reopen anything.
    pub fn close(&mut self) -> Vec<Effect> {
        self.closed = true;
        vec![Effect::Send(close_request(self.id)), Effect::Close]
    }

    pub fn key(&mut self, key: Key) -> Vec<Effect> {
        match key {
            Key::Escape | Key::Enter | Key::Space => self.close(),
            _ => Vec::new(),
        }
    }

    /// Sample input: {"t":"menuaction","op":"snapshot","id":9,"ok":true} or {"op":"snapshot","id":9,"ok":false,"error":"The folder changed; reopen the menu."}, then {"op":"properties","id":9,"ok":true,"directory":true,"bytes":4096} and {"op":"contents","id":9,"ok":true,"bytes":13,"items":1,"done":true,"partial":false}; a reply for another id is ignored.
    pub fn reply(&mut self, i18n: &I18n, reply: &Json) -> Vec<Effect> {
        if self.closed || reply.get("id").and_then(Json::as_f64) != Some(self.id as f64) {
            return Vec::new();
        }
        let ok = reply.get("ok").and_then(Json::as_bool) == Some(true);
        let error = reply.get("error").and_then(Json::as_str).unwrap_or("");
        match (reply.get("op").and_then(Json::as_str), &mut self.shown) {
            (Some("snapshot"), Shown::Asking) if !self.asked && !ok => {
                self.shown = Shown::Refused(logic::refusal(i18n, error))
            }
            (Some("snapshot"), Shown::Asking) if !self.asked => {
                self.asked = true;
                return vec![Effect::Send(logic::properties_request(self.id))];
            }
            (Some("properties"), _) if !ok => {
                self.shown = Shown::Refused(logic::refusal(i18n, error))
            }
            // A cancelled several-item walk ends quietly, never as an error toast.
            (Some("properties"), _)
                if self.count > 1
                    && reply.get("cancelled").and_then(Json::as_bool) == Some(true) =>
            {
                self.closed = true;
                return vec![Effect::Close];
            }
            (Some("properties"), _) if self.count > 1 => {
                self.shown = Shown::Several(Several::from_reply(reply))
            }
            (Some("properties"), _) => {
                let facts = Facts::from_reply(reply);
                let walk = facts.directory.then(|| logic::begin(&facts));
                let media = facts.asks_media();
                self.shown = Shown::One(facts, walk);
                if walk.is_some() {
                    return vec![Effect::Send(logic::contents_request(self.id))];
                }
                // A file's codecs and deferred EXIF follow on the same id, as v0.3.12 asked them.
                if media {
                    return vec![Effect::Send(logic::facts_request(self.id))];
                }
            }
            (Some("contents"), Shown::One(_, Some(walk))) => *walk = logic::next(*walk, reply),
            // A refused or cancelled facts reply adds no pair; the properties reply's own photo stays.
            (Some("facts"), Shown::One(_, None)) if ok => self.media = Some(logic::media(reply)),
            _ => {}
        }
        Vec::new()
    }

    pub fn card(&self, i18n: &I18n) -> Card {
        let (pairs, tags) = match &self.shown {
            Shown::One(facts, walk) => {
                let mut extras = self.extras.clone();
                extras.extend(logic::media_pairs(i18n, facts, self.media.as_ref()));
                (
                    logic::pairs(i18n, facts, *walk, &extras, &self.mime_kind),
                    facts.tags.clone(),
                )
            }
            Shown::Several(several) => (logic::several_pairs(i18n, several), Vec::new()),
            Shown::Asking | Shown::Refused(_) => (Vec::new(), Vec::new()),
        };
        let mut body = Vec::new();
        match &self.shown {
            Shown::Refused(text) => body.push(Block::Note {
                text: text.clone(),
                inset: 0.,
                ink: Ink::Error,
                lift: 0.,
            }),
            // Until the first reply lands the card says it is working, in the caption's muted ink, as v0.3.12 did.
            Shown::Asking => body.push(Block::Note {
                text: i18n.tr("dialogs.file.properties.working"),
                inset: 0.,
                ink: Ink::Muted,
                lift: 0.,
            }),
            _ => {}
        }
        if !pairs.is_empty() {
            let facts = pairs
                .into_iter()
                .map(|pair| match pair.name {
                    // Tags draw one by one, each after its colour mark, as the tagged board does.
                    Name::Own(Label::Tags) => Fact::Tags {
                        name: i18n.tr(Label::Tags.key()),
                        tags: tags.clone(),
                    },
                    Name::Own(label) => Fact::Text {
                        name: i18n.tr(label.key()),
                        value: pair.line.value,
                        tail: pair.line.tail,
                    },
                    Name::Extra(name) => Fact::Text {
                        name,
                        value: pair.line.value,
                        tail: pair.line.tail,
                    },
                })
                .collect();
            body.push(Block::Facts(facts));
        }
        Card {
            language: i18n.language().to_owned(),
            title: if self.count > 1 {
                i18n.trn("items.count", self.count as i64)
            } else {
                i18n.tr("dialogs.file.properties.title")
            },
            esc: i18n.tr("dialogs.file.esc"),
            width: WIDTH,
            bleed: false,
            body,
            buttons: vec![Button {
                label: i18n.tr("dialogs.file.properties.close"),
                primary: true,
                focused: true,
                enabled: true,
            }],
        }
    }
}
