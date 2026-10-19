//! The run card: three buttons for a program the opener refused to run unasked, with Shift relabelling Run in Terminal to Run.
use super::{
    keys::Key,
    shell::{Block, Button as Drawn, Card, Glyph},
    Effect,
};
use flea::i18n::I18n;
use flea::ui_logic_op3::runfile::{self as logic, Action, Ask, Button, Step};

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
}

impl RunCard {
    /// An ask line that does not parse was never checked, so the pane says so and nothing runs.
    pub fn open(i18n: &I18n, flea: &str, path: &str, line: &str) -> Result<Self, String> {
        let ask = logic::parse_ask(line).ok_or_else(|| i18n.tr("dialogs.file.run.unchecked"))?;
        let buttons = logic::buttons_for(ask.kind, ask.has_handler());
        let focus = logic::start_focus(&buttons, None);
        Ok(Self {
            flea: flea.to_string(),
            name: flea::ui_logic_op3::permissions::leaf_of(path).to_string(),
            ask,
            buttons,
            focus,
            shift: false,
        })
    }

    /// The listed row's mark name for a binary, for the icon service to resolve.
    pub fn glyph(&self, listed: &str) -> String {
        logic::glyph_for(self.ask.kind, listed)
    }

    /// The board draws a script's code mark; a binary keeps the listed row's mark once the icon service hands it over.
    fn mark(&self) -> Glyph {
        match self.ask.kind {
            flea::ui_logic_op3::runfile::Kind::Script => Glyph::Code,
            _ => Glyph::AppWindow,
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
