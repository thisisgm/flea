//! The Properties card: one item's two-column facts with a folder walk counted off the UI thread, or several items' summary.
use super::{
    keys::Key,
    shell::{Block, Button, Card, Fact, Ink},
    Effect,
};
use flea::i18n::I18n;
use flea::jsondoc::Json;
use flea::ui_logic_op3::openwith::close_request;
use flea::ui_logic_op3::properties::{self as logic, Facts, Label, Name, Several, Walk};

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
    several: bool,
    shown: Shown,
    /// The facts view's extra pairs (photo, codecs) and the MIME database's kind word, filled by their own producers.
    extras: Vec<(String, String)>,
    mime_kind: String,
}

impl Properties {
    /// The snapshot captures the selection first, so a later cursor move cannot change what is described.
    pub fn open(id: u64, rows: &[usize]) -> (Self, Vec<Effect>) {
        let card = Self {
            id,
            several: rows.len() > 1,
            shown: Shown::Asking,
            extras: Vec::new(),
            mime_kind: String::new(),
        };
        (
            card,
            vec![
                Effect::Send(logic::snapshot_request(id, rows)),
                Effect::Send(logic::properties_request(id)),
            ],
        )
    }

    pub fn set_extras(&mut self, extras: Vec<(String, String)>, mime_kind: &str) {
        self.extras = extras;
        self.mime_kind = mime_kind.to_string();
    }

    /// Closing cancels the old id, so a late reply for it cannot reopen anything.
    fn close(&self) -> Vec<Effect> {
        vec![Effect::Send(close_request(self.id)), Effect::Close]
    }

    pub fn key(&mut self, key: Key) -> Vec<Effect> {
        match key {
            Key::Escape | Key::Enter | Key::Space => self.close(),
            _ => Vec::new(),
        }
    }

    /// Sample input: a properties reply, then a folder's contents progress lines; a reply for another id is ignored.
    pub fn reply(&mut self, i18n: &I18n, reply: &Json) -> Vec<Effect> {
        if reply.get("id").and_then(Json::as_f64) != Some(self.id as f64) {
            return Vec::new();
        }
        let ok = reply.get("ok").and_then(Json::as_bool) == Some(true);
        let error = reply.get("error").and_then(Json::as_str).unwrap_or("");
        match (reply.get("op").and_then(Json::as_str), &mut self.shown) {
            (Some("properties"), _) if !ok => {
                self.shown = Shown::Refused(logic::refusal(i18n, error))
            }
            // A cancelled several-item walk ends quietly, never as an error toast.
            (Some("properties"), _)
                if self.several && reply.get("cancelled").and_then(Json::as_bool) == Some(true) =>
            {
                return vec![Effect::Close]
            }
            (Some("properties"), _) if self.several => {
                self.shown = Shown::Several(Several::from_reply(reply))
            }
            (Some("properties"), _) => {
                let facts = Facts::from_reply(reply);
                let walk = facts.directory.then(|| logic::begin(&facts));
                self.shown = Shown::One(facts, walk);
                if walk.is_some() {
                    return vec![Effect::Send(logic::contents_request(self.id))];
                }
            }
            (Some("contents"), Shown::One(_, Some(walk))) => *walk = logic::next(*walk, reply),
            _ => {}
        }
        Vec::new()
    }

    pub fn card(&self, i18n: &I18n) -> Card {
        let (pairs, tags) = match &self.shown {
            Shown::One(facts, walk) => (
                logic::pairs(i18n, facts, *walk, &self.extras, &self.mime_kind),
                facts.tags.clone(),
            ),
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
            title: i18n.tr("dialogs.file.properties.title"),
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
