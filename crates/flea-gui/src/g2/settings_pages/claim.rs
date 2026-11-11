//! The About default switch follows the queried desktop handler and keeps working until both follow-up jobs finish.
use flea::ui_logic_settings_pages::{
    rows::{Kind, Role, Row},
    Text,
};

pub const DESKTOP: &str = "com.thisisgm.flea.desktop";
#[derive(Clone, Default)]
pub struct Claim {
    pub handler: String,
    pub packaged: bool,
    pub running: bool,
    pub claiming: bool,
    pub note: Option<(Text, Role)>,
    pub revision: u64,
    pub probed: bool,
}
impl Claim {
    pub fn start(&mut self) -> Option<bool> {
        if self.running || !self.packaged {
            return None;
        }
        self.revision += 1;
        self.running = true;
        self.claiming = self.handler != DESKTOP;
        self.note = Some((
            Text::Key(if self.claiming {
                "settings.default.claiming"
            } else {
                "settings.default.releasing"
            }),
            Role::None,
        ));
        Some(self.claiming)
    }
    pub fn rows(&self) -> Vec<Row> {
        let mut out = vec![Row {
            id: "makeDefault".into(),
            on: self.handler == DESKTOP,
            inert: self.running || !self.packaged,
            ..Row::new(Kind::Check, Text::Key("settings.default.label"))
        }];
        let note = self.note.clone().or_else(|| {
            if self.probed && !self.packaged {
                Some((Text::Key("settings.default.uninstalled"), Role::Foreground))
            } else if self.handler == DESKTOP {
                Some((Text::Key("settings.default.on"), Role::Foreground))
            } else {
                None
            }
        });
        if let Some((label, role)) = note {
            out.push(Row {
                role,
                elide_right: true,
                ..Row::hint(label)
            });
        }
        out
    }
}
