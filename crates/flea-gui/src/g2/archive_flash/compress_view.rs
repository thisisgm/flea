//! Compress card presentation from the retained input model.
use super::*;

impl Compress {
    /// The card as the shell draws it: chips, name, the masked pair with eyes, the header switch, the buttons.
    pub fn card(&self, i18n: &I18n) -> Card {
        let error = self.error.clone().unwrap_or_default();
        let field = |label: &str,
                     value: &str,
                     placeholder: &str,
                     focused: bool,
                     error: bool,
                     revealed: bool| Field {
            input: None,
            label: Some(label.into()),
            value: value.into(),
            placeholder: placeholder.into(),
            secret: true,
            held: 0,
            revealed,
            eye: true,
            error,
            focused,
        };
        let current = self
            .formats
            .iter()
            .position(|f| *f == self.format)
            .unwrap_or(0);
        let (password_error, confirm_error) = self.field_errors();
        let mut body = vec![
            Block::Chips(Chips {
                options: self.formats.clone(),
                current,
                focused: self.focus == Stop::Format,
                label: i18n.tr("archive.format"),
            }),
            Block::Field(Field {
                input: None,
                label: Some(i18n.tr("archive.name")),
                value: self.name.clone(),
                placeholder: i18n.tr("archive.name_placeholder"),
                secret: false,
                held: 0,
                revealed: false,
                eye: false,
                error: false,
                focused: self.focus == Stop::Name,
            }),
            Block::Field(field(
                &i18n.tr("archive.password"),
                &self.password,
                "",
                self.focus == Stop::Password,
                password_error,
                self.held_password,
            )),
            Block::Field(field(
                &i18n.tr("archive.confirm"),
                &self.confirm,
                "",
                self.focus == Stop::Confirm,
                confirm_error,
                self.held_confirm,
            )),
        ];
        if !error.is_empty() {
            body.push(Block::Note {
                text: error,
                ink: Ink::Error,
            });
        }
        if self.format == "7z" {
            body.push(Block::CheckLine(CheckLine {
                checked: self.encrypt_names,
                focused: self.focus == Stop::Encrypt,
                label: i18n.tr("archive.encrypt_names"),
            }));
        }
        Card {
            language: i18n.language().into(),
            title: logic::compress_title(i18n, self.paths.len()),
            esc: i18n.tr("dialogs.file.esc"),
            width: WIDTH,
            body,
            body_layout: crate::g2::archive_flash::cards::BodyLayout::Compress,
            buttons: vec![
                Button {
                    label: i18n.tr("archive.cancel"),
                    primary: false,
                    focused: self.focus == Stop::Cancel,
                    enabled: true,
                    available: true,
                },
                Button {
                    label: i18n.tr("archive.compress.action"),
                    primary: true,
                    focused: self.focus == Stop::Compress,
                    enabled: true,
                    available: true,
                },
            ],
        }
    }
}
