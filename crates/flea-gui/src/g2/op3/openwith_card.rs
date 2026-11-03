//! The Open With card and its bounded application rows.
use super::{logic, shell, OpenWith, Seat, Stop, VIEWPORT_HEIGHT, VIEWPORT_ROWS, WIDTH};
use flea::i18n::I18n;
use shell::{AppRow, Block, Button, Card, Check, Cursor, Glyph, Ink, Tri};

/// The list's closing hairline, drawn under the viewport rows and inside its fixed height.
const LIST_RULE: f32 = 1.;

impl OpenWith {
    pub fn card(&self, i18n: &I18n) -> Card {
        let kind = self.catalogue.as_ref().map_or("", |c| c.kind.as_str());
        let search = shell::Field {
            input: None,
            label: None,
            value: self.query.clone(),
            placeholder: i18n.tr("dialogs.file.openwith.search"),
            glyph: Some(Glyph::Search),
            focused: self.focus == Stop::Search,
        };
        let mut body = vec![Block::Field(search)];
        if self.catalogue.is_some() && self.rows.is_empty() {
            body.push(Block::Apps(
                vec![AppRow::Empty {
                    text: logic::no_match(i18n, &self.filtered),
                    height: VIEWPORT_HEIGHT - LIST_RULE,
                }],
                0.,
            ));
        } else {
            let (rows, shown) = self.shown(i18n, kind);
            let chrome: f32 = rows.iter().map(AppRow::chrome).sum();
            let filler =
                (VIEWPORT_HEIGHT - LIST_RULE - shown as f32 * shell::APP_ROW - chrome).max(0.);
            body.push(Block::Apps(rows, filler));
        }
        let always = Check {
            value: if self.always { Tri::On } else { Tri::Off },
            focused: self.focus == Stop::Always,
        };
        body.push(Block::CheckLine {
            check: always,
            label: logic::always_label(i18n, kind),
        });
        body.push(Block::Note {
            text: i18n.tr("dialogs.file.openwith.undo_note"),
            inset: shell::CHECK + shell::ROW_GAP,
            ink: Ink::Muted,
            lift: 3.,
        });
        if let Some(error) = &self.error {
            body.push(Block::Note {
                text: error.clone(),
                inset: 0.,
                ink: Ink::Error,
                lift: 0.,
            });
        } else if self.opening {
            body.push(Block::Note {
                text: i18n.tr("dialogs.file.openwith.opening"),
                inset: 0.,
                ink: Ink::Muted,
                lift: 0.,
            });
        }
        Card {
            language: i18n.language().to_owned(),
            title: logic::title(i18n, &self.name),
            esc: i18n.tr("dialogs.file.esc"),
            width: WIDTH,
            bleed: true,
            body,
            buttons: vec![
                Button {
                    label: i18n.tr("dialogs.file.cancel"),
                    primary: false,
                    focused: self.focus == Stop::Cancel,
                    enabled: true,
                },
                Button {
                    label: i18n.tr("dialogs.file.openwith.open"),
                    primary: true,
                    focused: self.focus == Stop::Open,
                    enabled: self.can_open(),
                },
            ],
        }
    }

    /// Only the rows the viewport shows are built, seven applications from the first row the cursor keeps in view.
    fn shown(&self, i18n: &I18n, kind: &str) -> (Vec<AppRow>, usize) {
        let Some(catalogue) = &self.catalogue else {
            return (Vec::new(), 0);
        };
        let mut out = Vec::new();
        let mut apps = 0;
        let start = self.top.min(self.rows.len());
        for (at, row) in self.rows[start..].iter().enumerate() {
            if apps == VIEWPORT_ROWS {
                break;
            }
            out.push(match row {
                Seat::Eyebrow { group, .. } => AppRow::Eyebrow {
                    text: logic::eyebrow(i18n, *group, kind),
                    first: at == 0,
                },
                Seat::App {
                    app,
                    group,
                    at: seat,
                } => {
                    apps += 1;
                    let app = catalogue.app(*group, *app).cloned().unwrap_or_default();
                    let tail = if app.default {
                        i18n.tr("dialogs.file.openwith.default")
                    } else {
                        String::new()
                    };
                    let cursor = match (*seat == self.cursor, self.focus) {
                        (false, _) => Cursor::None,
                        (true, Stop::List) => Cursor::Focused,
                        (true, _) => Cursor::Waiting,
                    };
                    AppRow::App {
                        at: *seat,
                        icon: app.icon,
                        label: app.label,
                        tail,
                        cursor,
                    }
                }
            });
        }
        (out, apps)
    }
}
