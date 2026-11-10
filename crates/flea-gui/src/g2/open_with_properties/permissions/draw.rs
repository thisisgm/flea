//! The Permissions card drawn as plain data: the grid, the subject and octal lines, and the notes under them.
use super::{logic, Permissions, Phase, Stop, WIDTH};
use crate::g2::open_with_properties::shell::{Block, Button, Card, Check, Field, Glyph, Ink, Tri};
use flea::i18n::I18n;
use flea::ui_logic_open_with_properties::permissions::BoxValue;

impl Permissions {
    pub(super) fn boxes(&self) -> Vec<Tri> {
        let summary = self.summary();
        let bits = if self.several() {
            0
        } else {
            logic::parse(&self.text)
                .or_else(|| logic::shown_bits(&self.held.mode))
                .unwrap_or(0)
        };
        (0..9)
            .map(|b| {
                let bit = 1 << (8 - b);
                if self.several() {
                    match logic::multi_value(&summary, self.set, self.clear, bit) {
                        BoxValue::On => Tri::On,
                        BoxValue::Off => Tri::Off,
                        BoxValue::Some => Tri::Mixed,
                    }
                } else if bits & bit != 0 {
                    Tri::On
                } else {
                    Tri::Off
                }
            })
            .collect()
    }

    fn grid(&self, i18n: &I18n) -> Block {
        let tr = |key: &str| i18n.tr(key);
        let execute = if self.held.directory {
            "dialogs.file.permissions.enter"
        } else {
            "dialogs.file.permissions.execute"
        };
        let boxes = self.boxes();
        let focus = match self.focus {
            Stop::Box(b) => Some(b),
            _ => None,
        };
        let check = |b: usize| Check {
            value: boxes[b],
            focused: focus == Some(b),
        };
        let rows = ["owner", "group", "everyone"]
            .iter()
            .enumerate()
            .map(|(r, who)| {
                (
                    tr(&format!("dialogs.file.permissions.{who}")),
                    [check(r * 3), check(r * 3 + 1), check(r * 3 + 2)],
                )
            })
            .collect();
        Block::Grid {
            columns: vec![
                tr("dialogs.file.permissions.read"),
                tr("dialogs.file.permissions.write"),
                tr(execute),
            ],
            rows,
        }
    }

    fn note(text: String) -> Block {
        Block::Note {
            text,
            inset: 0.,
            ink: Ink::Foreground,
            lift: 0.,
        }
    }

    pub fn card(&self, i18n: &I18n) -> Card {
        let tr = |key: &str| i18n.tr(key);
        let mut body = Vec::new();
        let title = if self.several() {
            i18n.trn(
                "dialogs.file.permissions.title_many",
                self.paths.len() as i64,
            )
        } else {
            tr("dialogs.file.permissions.title")
        };
        let cancel = |focused: bool| Button {
            label: tr("dialogs.file.cancel"),
            primary: false,
            focused,
            enabled: self.phase != Phase::Applying,
        };
        match &self.phase {
            Phase::Stopped(applying) => {
                let key = if *applying {
                    "dialogs.file.permissions.stopped_applying"
                } else {
                    "dialogs.file.permissions.stopped"
                };
                body.push(Block::Note {
                    text: tr(key),
                    inset: 0.,
                    ink: Ink::Error,
                    lift: 0.,
                });
            }
            // A failed batch keeps its refusal visible while its modes are read again.
            Phase::Reading if self.error.is_empty() && self.refused.is_empty() => {
                return super::super::initial::loading(
                    i18n,
                    "dialogs.file.permissions.title",
                    WIDTH,
                )
            }
            Phase::Reading => body.push(Block::Note {
                text: tr("dialogs.file.permissions.reading"),
                inset: 0.,
                ink: Ink::Muted,
                lift: 0.,
            }),
            _ => {}
        }
        if self.several() && self.phase != Phase::Reading {
            body.push(self.grid(i18n));
            // PermissionsDialog.qml's scopeLabel says it on every several-item card, mixed boxes or not.
            body.push(Self::note(tr("dialogs.file.permissions.mixed")));
            let note = self.store.note_text(i18n, &self.paths);
            if !note.is_empty() {
                body.push(Block::Note {
                    text: note,
                    inset: 0.,
                    ink: Ink::Error,
                    lift: 0.,
                });
            }
        } else if self.phase != Phase::Reading {
            let kind = if !self.kind.is_empty() {
                self.kind.clone()
            } else if self.held.directory {
                tr("dialogs.file.permissions.kind_folder")
            } else {
                tr("dialogs.file.permissions.kind_file")
            };
            let owner = if self.held.owner.is_empty() {
                tr("dialogs.file.permissions.unknown")
            } else {
                self.held.owner.clone()
            };
            let group = if self.held.group.is_empty() {
                tr("dialogs.file.permissions.unknown")
            } else {
                self.held.group.clone()
            };
            let scope = if self.held.directory {
                "dialogs.file.permissions.scope_folder"
            } else {
                "dialogs.file.permissions.scope_item"
            };
            let octal = Field {
                input: None,
                label: Some(tr("dialogs.file.permissions.octal")),
                value: self.text.clone(),
                placeholder: String::new(),
                glyph: None,
                focused: self.focus == Stop::Octal,
            };
            let mut stack = vec![
                Block::Subject {
                    glyph: if self.held.directory {
                        Glyph::Folder
                    } else {
                        Glyph::File
                    },
                    name: logic::leaf_of(&self.held.path).to_string(),
                    kind,
                },
                Block::Rule {
                    above: 0.,
                    below: 0.,
                },
                self.grid(i18n),
                Block::Rule {
                    above: 5.,
                    below: 12.,
                },
                Block::Field(octal),
                Block::Info {
                    label: tr("dialogs.file.permissions.owner"),
                    value: i18n.tr_args("dialogs.file.permissions.read_only", &[&owner]),
                    tail: i18n.tr_args(
                        "dialogs.file.permissions.uid",
                        &[&self.held.uid.to_string()],
                    ),
                },
                Block::Info {
                    label: tr("dialogs.file.permissions.group"),
                    value: i18n.tr_args("dialogs.file.permissions.read_only", &[&group]),
                    tail: i18n.tr_args(
                        "dialogs.file.permissions.gid",
                        &[&self.held.gid.to_string()],
                    ),
                },
            ];
            // v0.3.12 PermissionsDialog.qml:551-568: a folder's scope box sits flush under Owner and Group.
            if self.held.directory {
                stack.push(Block::Scope(tr(
                    "dialogs.file.permissions.scope_folder_note",
                )));
            }
            // v0.3.12 PermissionsDialog.qml:602-606: only an editable card with a whole mode requests one; a refused card names the mode it keeps.
            let mut lines = vec![
                if self.changeable() && logic::parse(&self.text).is_some() {
                    i18n.tr_args("dialogs.file.permissions.requested", &[&self.text])
                } else {
                    tr("dialogs.file.permissions.no_changes")
                },
                i18n.tr_args("dialogs.file.permissions.path", &[&self.held.path]),
            ];
            if !self.held.reason.is_empty() {
                lines.push(
                    i18n.tr_args("dialogs.file.permissions.current_mode", &[&self.held.mode]),
                );
            }
            lines.push(tr(scope));
            stack.extend([
                Block::Rule {
                    above: 11.,
                    below: 9.,
                },
                Block::Section {
                    head: tr("dialogs.file.permissions.will_change"),
                    lines,
                },
            ]);
            body.push(Block::Stack(stack));
            // A read-only card holds the file's own mode, a four-digit one included, so only an editable one names a typing error.
            if self.changeable() && logic::parse(&self.text).is_none() {
                body.push(Block::Note {
                    text: tr("dialogs.file.permissions.octal_invalid"),
                    inset: 0.,
                    ink: Ink::Error,
                    lift: 0.,
                });
            }
            if !self.held.reason.is_empty() {
                body.push(Block::Note {
                    text: logic::closed(&self.held.reason),
                    inset: 0.,
                    ink: Ink::Error,
                    lift: 0.,
                });
            }
        }
        if self.phase == Phase::Applying {
            body.push(Block::Note {
                text: tr("dialogs.file.permissions.applying"),
                inset: 0.,
                ink: Ink::Muted,
                lift: 0.,
            });
        }
        if !self.error.is_empty() {
            body.push(Block::Note {
                text: self.error.clone(),
                inset: 0.,
                ink: Ink::Error,
                lift: 0.,
            });
        }
        Card {
            language: i18n.language().to_owned(),
            title,
            esc: tr("dialogs.file.esc"),
            width: WIDTH,
            bleed: false,
            body,
            buttons: vec![
                cancel(self.focus == Stop::Cancel),
                Button {
                    label: tr("dialogs.file.apply"),
                    primary: true,
                    focused: self.focus == Stop::Apply,
                    enabled: self.can_apply(),
                },
            ],
        }
    }
}
