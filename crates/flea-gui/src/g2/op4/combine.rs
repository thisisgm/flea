//! Live composition settings and reordering; no image decoder belongs to this card.
use super::{
    model::{Card, Focus, Model},
    seam::Ticket,
};
use flea::{
    i18n::I18n,
    ui_logic_op4::{
        composition::{self, Background, Layout, Settings, Spacing},
        output,
        selection::Selection,
    },
};
use std::path::Path;

pub struct Combine {
    pub settings: Settings,
    pub format: String,
    pub name: String,
    pub custom_name: bool,
    pub columns_text: String,
    pub focused: Option<usize>,
    pub pending: Option<Ticket>,
    pub collision: bool,
}

impl Combine {
    pub fn new(selection: &Selection) -> Self {
        let first = Path::new(&selection.files[0].path);
        let format = output::combined_format(first).to_string();
        Self {
            name: output::combined_name(first, &format),
            custom_name: false,
            columns_text: "2".into(),
            settings: Settings {
                layout: Layout::Row,
                spacing: Spacing::None,
                background: if format == "jpg" {
                    Background::White
                } else {
                    Background::Transparent
                },
                columns: 2,
            },
            format,
            focused: None,
            pending: None,
            collision: false,
        }
    }

    pub fn grid(&self) -> bool {
        self.settings.layout == Layout::Grid
    }
    pub fn background_enabled(&self, count: usize) -> bool {
        composition::background_enabled(
            self.settings.layout,
            self.settings.spacing,
            count,
            self.settings.columns,
        )
    }
    pub fn output(&self, selection: &Selection) -> Result<String, &'static str> {
        if self.grid()
            && !self
                .columns_text
                .parse::<usize>()
                .ok()
                .is_some_and(|columns| (1..=output::MAX_ITEMS).contains(&columns))
        {
            return Err("dialogs.output.error.columns");
        }
        if self.format == "jpg" && self.settings.background == Background::Transparent {
            return Err("dialogs.output.error.jpeg_alpha");
        }
        if !output::bare_name(&self.name) || output::extension(Path::new(&self.name)) != self.format
        {
            return Err("dialogs.output.error.name");
        }
        Ok(Path::new(&selection.files[0].path)
            .with_file_name(&self.name)
            .to_string_lossy()
            .into_owned())
    }
}

impl Model {
    pub fn setting(&mut self, focus: Focus, index: usize, catalog: &I18n) {
        if !self.editable() {
            return;
        }
        let count = self.selection.as_ref().map_or(0, |s| s.files.len());
        let Some(Card::Combine(c)) = &mut self.card else {
            return;
        };
        match focus {
            Focus::Layout => {
                c.settings.layout = match index {
                    0 => Layout::Row,
                    1 => Layout::Column,
                    2 => Layout::Grid,
                    _ => return,
                }
            }
            Focus::Spacing => {
                c.settings.spacing = match index {
                    0 => Spacing::None,
                    1 => Spacing::Small,
                    2 => Spacing::Large,
                    _ => return,
                }
            }
            Focus::Background if c.background_enabled(count) => {
                let choices: &[Background] = if c.format == "jpg" {
                    &[Background::White, Background::Black]
                } else {
                    &[
                        Background::Transparent,
                        Background::White,
                        Background::Black,
                    ]
                };
                let Some(value) = choices.get(index) else {
                    return;
                };
                c.settings.background = *value;
            }
            Focus::Format => {
                let Some(format) = output::COMBINE_FORMATS.get(index) else {
                    return;
                };
                c.format = (*format).into();
                if *format == "jpg" && c.settings.background == Background::Transparent {
                    c.settings.background = Background::White;
                }
                if !c.custom_name {
                    c.name = output::combined_name(
                        Path::new(&self.selection.as_ref().unwrap().files[0].path),
                        format,
                    );
                }
            }
            _ => return,
        }
        self.focus = Some(focus);
        self.probe(catalog);
    }

    pub fn move_image(&mut self, from: usize, to: usize, catalog: &I18n) {
        if !self.editable() {
            return;
        }
        let Some(Card::Combine(c)) = &mut self.card else {
            return;
        };
        let Some(selection) = &mut self.selection else {
            return;
        };
        if from >= selection.files.len() || to >= selection.files.len() {
            return;
        }
        let file = selection.files.remove(from);
        selection.files.insert(to, file);
        c.focused = Some(to);
        self.focus = Some(Focus::Preview);
        if !c.custom_name {
            c.name = output::combined_name(Path::new(&selection.files[0].path), &c.format);
        }
        self.probe(catalog);
    }

    pub fn focus_image(&mut self, delta: isize) {
        if !self.editable() {
            return;
        }
        let Some(Card::Combine(c)) = &mut self.card else {
            return;
        };
        let count = self.selection.as_ref().map_or(0, |s| s.files.len());
        if count == 0 {
            return;
        }
        c.focused = Some(
            c.focused
                .map_or(0, |i| i.saturating_add_signed(delta).min(count - 1)),
        );
        self.focus = Some(Focus::Preview);
    }
}
