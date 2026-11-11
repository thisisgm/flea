//! Keyboard and text edits for the output card focus loop.
use super::{
    controls::Control,
    host::OutputView,
    model::{Card, Focus},
};
use gpui::{Context, KeyDownEvent, Window};
impl OutputView {
    pub fn edit(&mut self, text: &str, backspace: bool, cx: &mut Context<Self>) {
        if !self.model.editable() {
            return;
        }
        let field = match (&mut self.model.card, self.model.focus) {
            (Some(Card::Print(p)), Some(Focus::Copies)) => &mut p.copies,
            (Some(Card::Print(p)), Some(Focus::Pages)) => &mut p.pages,
            (Some(Card::Convert(c)), Some(Focus::Pages)) => &mut c.pages,
            (Some(Card::Combine(c)), Some(Focus::Name)) => {
                c.custom_name = true;
                &mut c.name
            }
            (Some(Card::Combine(c)), Some(Focus::Columns)) if c.grid() => &mut c.columns_text,
            _ => return,
        };
        if self.select_all {
            field.clear();
            self.select_all = false;
        }
        if backspace {
            field.pop();
        } else if field.len() + text.len() <= 4096 && !text.chars().any(char::is_control) {
            field.push_str(text);
        }
        self.validate_edit(cx);
    }

    pub(super) fn validate_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(Card::Combine(c)) = &mut self.model.card {
            if let Some(columns) = c
                .columns_text
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=256).contains(n))
            {
                c.settings.columns = columns;
            }
        }
        self.model.clear_error();
        if matches!(self.model.card, Some(Card::Print(_))) {
            if let Some(Card::Print(p)) = &self.model.card {
                if !p
                    .copies
                    .parse::<usize>()
                    .ok()
                    .is_some_and(|n| (1..=999).contains(&n))
                {
                    self.model
                        .set_error(&self.catalog, "dialogs.output.error.copies", &[]);
                } else if flea::ui_logic_print_convert::pages::Pages::parse(&p.pages).is_err() {
                    self.model
                        .set_error(&self.catalog, "dialogs.output.error.pages", &[]);
                }
            }
        } else {
            self.model.probe(&self.catalog);
        }
        self.flush(cx);
    }

    pub(super) fn key(&mut self, event: &KeyDownEvent, window: &Window, cx: &mut Context<Self>) {
        if self.model.card.is_none() {
            return;
        }
        if self.printer_menu.opened() {
            cx.stop_propagation();
            self.printer_key(event, window, cx);
            return;
        }
        self.sync_editor();
        if self.field_key(event, window, cx) {
            return;
        }
        cx.stop_propagation();
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.model.cancel();
            self.flush(cx);
            return;
        }
        if self.model.busy {
            return;
        }
        if key == "tab" {
            self.body_scroll.request_reveal();
            self.model.tab(event.keystroke.modifiers.shift);
            self.select_all = false;
            self.sync_editor();
            cx.notify();
            return;
        }
        if !self.model.editable() {
            if matches!(key, "enter" | "space") {
                self.model.close();
                self.flush(cx);
            }
            return;
        }
        if event.keystroke.modifiers.control && key == "a" {
            self.select_all = true;
            return;
        }
        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.platform
        {
            return;
        }
        if event.is_held && matches!(key, "enter" | "space") {
            return;
        }
        self.body_scroll.request_reveal();
        if matches!(key, "enter" | "space") && self.model.focus == Some(Focus::Printer) {
            self.activate(Control::Field(Focus::Printer), window, cx);
        } else if matches!(key, "enter" | "space") && self.model.focus == Some(Focus::Cancel) {
            self.model.close();
        } else if matches!(key, "enter" | "space") && self.model.focus == Some(Focus::Submit) {
            self.model.submit(&self.catalog);
        } else if key == "enter" {
            if self.model.focus == Some(Focus::Cancel) {
                self.model.close();
            } else if matches!(
                self.model.focus,
                Some(Focus::Check | Focus::Duplex | Focus::Grayscale)
            ) {
                self.activate(Control::Check(self.model.focus.unwrap()), window, cx);
            } else {
                self.model.submit(&self.catalog);
            }
        } else if key == "space"
            && matches!(
                self.model.focus,
                Some(Focus::Formats | Focus::Check | Focus::Duplex | Focus::Grayscale)
            )
        {
            let was_formats = self.model.focus == Some(Focus::Formats);
            let focus = if was_formats {
                Focus::Check
            } else {
                self.model.focus.unwrap()
            };
            self.activate(Control::Check(focus), window, cx);
            if was_formats {
                self.model.focus = Some(Focus::Formats);
            }
        } else if self.model.focus == Some(Focus::Preview) {
            self.preview_key(key);
        } else if matches!(self.model.card, Some(Card::Convert(_)))
            && self.model.focus != Some(Focus::Pages)
            && matches!(key, "up" | "down")
        {
            if let Some(Card::Convert(c)) = &self.model.card {
                let at = c.cursor;
                let at = if key == "up" {
                    at.saturating_sub(1)
                } else {
                    (at + 1).min(c.kind.formats().len() - 1)
                };
                self.model.choose_format(at, &self.catalog);
            }
        } else if matches!(key, "left" | "right" | "up" | "down") {
            self.choice_key(key);
        } else if key == "space" {
            self.edit(" ", false, cx);
        } else if key == "backspace" {
            self.edit("", true, cx);
        } else if let Some(text) = &event.keystroke.key_char {
            self.edit(text, false, cx);
        }
        self.flush(cx);
    }

    fn preview_key(&mut self, key: &str) {
        let Some(Card::Combine(c)) = &self.model.card else {
            return;
        };
        let columns = if c.grid() {
            c.settings.columns.max(1) as isize
        } else {
            1
        };
        let delta = match key {
            "left" | "h" | "[" => -1,
            "right" | "l" | "]" => 1,
            "up" | "k" => -columns,
            "down" | "j" => columns,
            _ => return,
        };
        if matches!(key, "[" | "]") {
            let at = c.focused.unwrap_or(0);
            self.model
                .move_image(at, at.saturating_add_signed(delta), &self.catalog);
        } else {
            self.model.focus_image(delta);
        }
    }

    fn choice_key(&mut self, key: &str) {
        let step = |index: usize, count: usize| {
            if matches!(key, "left" | "up") {
                index.saturating_sub(1)
            } else {
                (index + 1).min(count.saturating_sub(1))
            }
        };
        if let Some(Card::Print(p)) = &self.model.card {
            if self.model.focus == Some(Focus::Printer) && !p.destinations.is_empty() {
                let at = p
                    .destinations
                    .iter()
                    .position(|d| d.name == p.printer)
                    .unwrap_or(0);
                self.model
                    .query_printer(Some(step(at, p.destinations.len())));
            }
            return;
        }
        let (Some(Card::Combine(c)), Some(focus)) = (&self.model.card, self.model.focus) else {
            return;
        };
        use flea::ui_logic_print_convert::{
            composition::{Background, Layout, Spacing},
            output,
        };
        let (at, count) = match focus {
            Focus::Layout => (
                match c.settings.layout {
                    Layout::Row => 0,
                    Layout::Column => 1,
                    Layout::Grid => 2,
                },
                3,
            ),
            Focus::Spacing => (
                match c.settings.spacing {
                    Spacing::None => 0,
                    Spacing::Small => 1,
                    Spacing::Large => 2,
                },
                3,
            ),
            Focus::Format => (
                output::COMBINE_FORMATS
                    .iter()
                    .position(|f| *f == c.format)
                    .unwrap_or(0),
                output::COMBINE_FORMATS.len(),
            ),
            Focus::Background => {
                let grounds: &[Background] = if c.format == "jpg" {
                    &[Background::White, Background::Black]
                } else {
                    &[
                        Background::Transparent,
                        Background::White,
                        Background::Black,
                    ]
                };
                (
                    grounds
                        .iter()
                        .position(|g| *g == c.settings.background)
                        .unwrap_or(0),
                    grounds.len(),
                )
            }
            _ => return,
        };
        self.model.setting(focus, step(at, count), &self.catalog);
    }
}
