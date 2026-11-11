//! The run card: three buttons for a program the opener refused to run unasked, with Shift relabelling Run in Terminal to Run.
use super::{
    keys::Key,
    shell::{Block, Button as Drawn, Card, Glyph},
    Effect,
};
use flea::i18n::I18n;
use flea::ui_logic_open_with_properties::runfile::{self as logic, Action, Ask, Button, Step};

/// The board's 414 content at its 13 body, carried to 14, with its padding and hairlines.
pub const WIDTH: f32 = 500.;

#[derive(Clone, Debug)]
pub struct RunCard {
    flea: String,
    name: String,
    ask: Ask,
    buttons: Vec<Button>,
    pub focus: Button,
    pub shift: bool,
    listed: String,
}

impl RunCard {
    /// An ask line that does not parse was never checked, so the pane says so and nothing runs.
    pub fn open(i18n: &I18n, flea: &str, path: &str, line: &str) -> Result<Self, String> {
        let ask = logic::parse_ask(line).ok_or_else(|| i18n.tr("dialogs.file.run.unchecked"))?;
        let buttons = logic::buttons_for(ask.kind, ask.has_handler());
        let focus = logic::start_focus(&buttons, None);
        Ok(Self {
            flea: flea.to_string(),
            name: flea::ui_logic_open_with_properties::permissions::leaf_of(path).to_string(),
            ask,
            buttons,
            focus,
            shift: false,
            listed: String::new(),
        })
    }

    /// The listed row's mark name, read once at open as v0.3.12 RunFile.listedGlyph reads it; a binary keeps it, a script keeps the code mark.
    pub fn set_listed(&mut self, listed: &str) {
        self.listed = listed.to_string();
    }

    /// The listed row's mark name for a binary, for the icon service to resolve.
    pub fn glyph(&self, listed: &str) -> String {
        logic::glyph_for(self.ask.kind, listed)
    }

    /// The board draws a script's code mark; a binary keeps the listed row's mark the host read at open.
    fn mark(&self) -> Glyph {
        match self.glyph(&self.listed).as_str() {
            "code" => Glyph::Code,
            "terminal" => Glyph::Terminal,
            _ => Glyph::AppWindow,
        }
    }

    /// The button a drawn index names, so a click presses the same button Enter on it would.
    pub fn button_at(&self, at: usize) -> Option<Button> {
        self.buttons.get(at).copied()
    }

    /// A click on the drawn button at an index.
    pub fn press_at(&mut self, at: usize) -> Vec<Effect> {
        match self.button_at(at) {
            Some(button) => self.press(button),
            None => Vec::new(),
        }
    }

    pub fn press(&mut self, button: Button) -> Vec<Effect> {
        self.focus = button;
        let action = match button {
            Button::Cancel => return vec![Effect::Close],
            Button::Text => Action::Text,
            Button::Handler => Action::Handler,
            Button::Terminal if self.shift => Action::Run,
            Button::Terminal => Action::Terminal,
        };
        vec![
            Effect::Launch(logic::argv(&self.flea, action, &self.ask)),
            Effect::Close,
        ]
    }

    /// The action a launch ran, so its exit is read with the right sentence.
    pub fn action_of(&self, button: Button) -> Option<Action> {
        match button {
            Button::Cancel => None,
            Button::Text => Some(Action::Text),
            Button::Handler => Some(Action::Handler),
            Button::Terminal if self.shift => Some(Action::Run),
            Button::Terminal => Some(Action::Terminal),
        }
    }

    pub fn key(&mut self, key: Key) -> Vec<Effect> {
        let step = match key {
            Key::Escape => return vec![Effect::Close],
            Key::Enter => return self.press(self.focus),
            Key::Left => Step::Left,
            Key::Right => Step::Right,
            Key::Tab => Step::Tab,
            Key::Backtab => Step::Backtab,
            Key::Text(ref t) if t == "h" => Step::Left,
            Key::Text(ref t) if t == "l" => Step::Right,
            _ => return Vec::new(),
        };
        self.focus = logic::moved(&self.buttons, self.focus, step);
        Vec::new()
    }

    pub fn card(&self, i18n: &I18n) -> Card {
        let default = logic::default_button(self.ask.kind, self.ask.has_handler());
        Card {
            language: i18n.language().to_owned(),
            title: logic::title(i18n, &self.name),
            esc: i18n.tr("dialogs.file.esc"),
            width: WIDTH,
            bleed: false,
            body: vec![Block::Program {
                glyph: self.mark(),
                text: logic::subtitle(i18n, self.ask.kind, self.ask.size),
            }],
            buttons: self
                .buttons
                .iter()
                .map(|b| Drawn {
                    label: logic::label_for(i18n, *b, &self.ask.handler_name, self.shift),
                    primary: *b == default,
                    focused: *b == self.focus,
                    enabled: true,
                })
                .collect(),
        }
    }
}

impl RunCard {
    /// RunCard.qml's runCardState: what the card asks, its buttons, and where the latest frame laid them out.
    pub(crate) fn ipc_state(
        &self,
        i18n: &I18n,
        geometry: &crate::ipc::card_geometry::CardGeometry,
    ) -> String {
        use flea::jsondoc::Json;
        let probe = &geometry.probe;
        let card = self.card(i18n);
        let text = |value: &str| Json::Str(value.into());
        let pointer = |id: &str, field: &str| {
            geometry
                .control(id, "", false, true)
                .get(field)
                .cloned()
                .unwrap_or(Json::Bool(false))
        };
        let controls =
            card.buttons
                .iter()
                .zip(&self.buttons)
                .enumerate()
                .map(|(at, (drawn, button))| {
                    let id = format!("button:{at}");
                    crate::visual::object([
                        ("name", text(button.name())),
                        ("label", text(&drawn.label)),
                        ("primary", Json::Bool(drawn.primary)),
                        ("focused", Json::Bool(drawn.focused)),
                        ("hovered", pointer(&id, "hovered")),
                        ("pressed", pointer(&id, "pressed")),
                        ("centre", Json::Str(probe.centre(&id, &id))),
                        ("rect", Json::Str(probe.rect(&id))),
                    ])
                });
        let default = logic::default_button(self.ask.kind, self.ask.has_handler());
        let state = crate::visual::object([
            ("opened", Json::Bool(true)),
            ("title", Json::Str(card.title.clone())),
            ("kind", text(self.ask.kind.name())),
            (
                "buttons",
                Json::Arr(self.buttons.iter().map(|b| text(b.name())).collect()),
            ),
            ("focus", text(self.focus.name())),
            ("default", text(default.name())),
            ("rect", Json::Str(probe.rect("card"))),
            ("controls", Json::Arr(controls.collect())),
        ]);
        flea::jsondoc::render(&state).trim_end().into()
    }
}

#[cfg(test)]
mod ipc_tests {
    use super::*;

    #[test]
    fn observer_keeps_stable_ids_beside_the_drawn_catalog_labels() {
        let i18n = crate::g2::open_with_properties::tests::english();
        let card = RunCard::open(&i18n, "flea", "/tmp/tool.sh", r#"{"ask":"run","kind":"script","size":21,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":1,"mtimeNs":2,"target":"/tmp/tool.sh"}"#).unwrap();
        let state = card.ipc_state(&i18n, &Default::default());
        let state = flea::jsondoc::parse(&state).unwrap();
        assert_eq!(
            state.get("kind").and_then(flea::jsondoc::Json::as_str),
            Some("script")
        );
        assert_eq!(
            state.get("focus").and_then(flea::jsondoc::Json::as_str),
            Some("terminal")
        );
        let buttons = state
            .get("buttons")
            .and_then(flea::jsondoc::Json::as_array)
            .unwrap();
        assert_eq!(
            buttons,
            &["cancel", "text", "terminal"].map(|s| flea::jsondoc::Json::Str(s.into()))
        );
        let controls = state
            .get("controls")
            .and_then(flea::jsondoc::Json::as_array)
            .unwrap();
        let names: Vec<_> = controls
            .iter()
            .map(|control| control.get("name").and_then(flea::jsondoc::Json::as_str))
            .collect();
        assert_eq!(names, [Some("cancel"), Some("text"), Some("terminal")]);
        let drawn = card.card(&i18n).buttons;
        assert_eq!(drawn.len(), controls.len());
        for ((actual, drawn), name) in controls.iter().zip(drawn).zip(names) {
            assert_eq!(
                actual.get("label").and_then(flea::jsondoc::Json::as_str),
                Some(drawn.label.as_str())
            );
            assert_ne!(
                Some(drawn.label.as_str()),
                name,
                "a drawn label must not stand in for its id"
            );
        }
    }
}
