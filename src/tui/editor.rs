use super::{
    input::Key,
    render,
    theme::{Depth, Theme},
};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

pub struct Editor {
    pub kind: &'static str,
    pub value: String,
    pub path: PathBuf,
    pub pending: bool,
    pub default_base: Option<String>,
    pub error: String,
    error_text: Option<(super::words::Text, String)>,
    cursor: usize,
    anchor: usize,
    editable_end: usize,
    full_name: bool,
    identity: Option<(u64, u64, u32)>,
}
impl Editor {
    pub fn new(kind: &'static str, value: String, path: PathBuf) -> Self {
        let cursor = value.len();
        Self {
            kind,
            value,
            path,
            cursor,
            anchor: cursor,
            editable_end: cursor,
            full_name: true,
            pending: false,
            default_base: None,
            error: String::new(),
            error_text: None,
            identity: None,
        }
    }
    // New Folder and New File open with their whole suggested name selected, so typing replaces it.
    pub fn named(kind: &'static str, value: String, path: PathBuf) -> Self {
        let mut editor = Self::new(kind, value, path);
        editor.anchor = 0;
        editor.default_base = (kind == "mkdir").then(|| editor.value.clone());
        editor
    }
    pub fn rename(value: String, path: PathBuf, directory: bool) -> std::io::Result<Self> {
        let meta = path.symlink_metadata()?;
        let mut editor = Self::new("rename", value, path);
        editor.identity = Some((meta.dev(), meta.ino(), meta.mode() & 0o170000));
        editor.cursor = if directory {
            editor.value.len()
        } else {
            editor
                .value
                .rfind('.')
                .filter(|at| *at > 0)
                .unwrap_or(editor.value.len())
        };
        editor.anchor = 0;
        editor.editable_end = editor.cursor;
        editor.full_name = editor.cursor == editor.value.len();
        Ok(editor)
    }
    pub fn fail_data(&mut self, error: String) {
        self.error_text = None;
        self.error = error;
    }
    pub fn refresh_locale(&mut self, i18n: &crate::i18n::I18n) {
        if let Some((text, before)) = &mut self.error_text {
            if self.error == *before {
                self.error = text.render(i18n);
                *before = self.error.clone();
            } else {
                self.error_text = None;
            }
        }
    }
    fn fail_key(&mut self, key: &'static str, i18n: &crate::i18n::I18n) {
        let text = super::words::Text::Key(key);
        self.error = text.render(i18n);
        self.error_text = Some((text, self.error.clone()));
    }
    #[cfg(test)]
    pub fn valid(&mut self) -> bool {
        self.valid_localized(&crate::i18n::test_catalogs::english())
    }
    pub fn valid_localized(&mut self, i18n: &crate::i18n::I18n) -> bool {
        if !self.full_name && self.editable_end == 0 {
            self.fail_key("tui.editor.enter_a_filename_before_the_extension", i18n);
            return false;
        }
        // One prompt makes a file or a folder: a trailing slash names a folder, so the name the
        // shared validator reads drops that one slash and nothing else. A slash anywhere else is
        // still refused, which is why the name, not the draft, is what gets validated.
        let made = matches!(self.kind, "rename" | "mkdir" | "newfile");
        let name = match self.value.strip_suffix('/') {
            Some(name) if made && self.kind != "rename" => name,
            _ => self.value.as_str(),
        };
        if made && crate::backend::ops::name_refusal(name).is_some() {
            // The shared validator decides refusal; input facts select only its presentation key.
            let key = if name.is_empty() {
                "tui.editor.name_empty"
            } else if matches!(name, "." | "..") {
                "tui.editor.name_dots"
            } else if self.value.contains('/') {
                "tui.editor.name_slash"
            } else if name.contains('\0') {
                "tui.editor.name_nul"
            } else {
                "tui.editor.invalid_name"
            };
            self.fail_key(key, i18n);
            return false;
        }
        if self.kind == "rename" {
            match self.path.symlink_metadata() {
                Ok(meta)
                    if self.identity == Some((meta.dev(), meta.ino(), meta.mode() & 0o170000)) => {}
                _ => {
                    self.fail_key("tui.editor.selected_item_changed_reopen_rename", i18n);
                    return false;
                }
            }
        }
        true
    }
    pub fn replace(&mut self, value: String) {
        self.value = value;
        self.cursor = self.value.len();
        self.anchor = self.cursor;
        self.editable_end = self.cursor;
        self.full_name = true;
    }
    pub fn update(&mut self, key: &Key) {
        let shift = key.mods.contains("shift");
        let command = matches!(key.mods.as_str(), "ctrl" | "super");
        if command && key.name == "A" {
            self.full_name = true;
            self.editable_end = self.value.len();
            self.anchor = 0;
            self.cursor = self.value.len();
            return;
        }
        let old_length = self.value.len();
        if self.word_command(key) {
            self.editable_end = self.editable_end + self.value.len() - old_length;
            self.error.clear();
            self.error_text = None;
            return;
        }
        let start = self.cursor.min(self.anchor);
        let end = self.cursor.max(self.anchor);
        let previous = self.value[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i);
        let next = (self.cursor
            + self.value[self.cursor..]
                .chars()
                .next()
                .map_or(0, char::len_utf8))
        .min(self.editable_end);
        let old_length = self.value.len();
        match key.name.as_str() {
            "Left" | "Right" | "Home" | "End" => {
                self.cursor = match key.name.as_str() {
                    "Home" => 0,
                    "End" => self.editable_end,
                    "Left" => {
                        if !shift && start != end {
                            start
                        } else {
                            previous
                        }
                    }
                    _ => {
                        if !shift && start != end {
                            end
                        } else {
                            next
                        }
                    }
                };
                if !shift {
                    self.anchor = self.cursor;
                }
            }
            "Backspace" | "Delete" if key.mods.is_empty() => {
                let range = if start != end {
                    start..end
                } else if key.name == "Backspace" {
                    previous..self.cursor
                } else {
                    self.cursor..next
                };
                self.cursor = range.start;
                self.value.replace_range(range, "");
                self.anchor = self.cursor;
            }
            _ if matches!(key.mods.as_str(), "" | "shift") && !key.text.is_empty() => {
                let text = render::clean(&key.text);
                self.value.replace_range(start..end, &text);
                self.cursor = start + text.len();
                self.anchor = self.cursor;
            }
            _ => {}
        }
        self.editable_end = self.editable_end + self.value.len() - old_length;
        self.error.clear();
        self.error_text = None;
    }
    fn word_command(&mut self, key: &Key) -> bool {
        use super::editor_words::{backward, forward, kill_length};
        let mods = key.mods.as_str();
        let motion = match (mods, key.name.as_str()) {
            ("alt", "B") | ("ctrl" | "ctrlshift", "Left") => Some(false),
            ("alt", "F") | ("ctrl" | "ctrlshift", "Right") => Some(true),
            _ => None,
        };
        if let Some(right) = motion {
            self.cursor = if right {
                forward(&self.value, self.cursor, self.editable_end)
            } else {
                backward(&self.value, self.cursor)
            };
            if mods != "ctrlshift" {
                self.anchor = self.cursor;
            }
            return true;
        }
        let range = match (mods, key.name.as_str()) {
            ("ctrl", "U") => 0..self.cursor,
            ("ctrl", "K") => self.cursor..self.editable_end,
            ("ctrl", "W" | "Backspace") => {
                self.cursor - kill_length(self.value[..self.cursor].chars().rev())..self.cursor
            }
            ("alt", "D") | ("ctrl", "Delete") => {
                self.cursor
                    ..self.cursor + kill_length(self.value[self.cursor..self.editable_end].chars())
            }
            _ => return false,
        };
        let range = if self.cursor != self.anchor {
            self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
        } else {
            range
        };
        self.cursor = range.start;
        self.anchor = self.cursor;
        self.value.replace_range(range, "");
        true
    }
    /// Draws the status-row field and returns its cells and caret cell; unsafe characters are filtered at paint time, so the stored name stays whole.
    pub fn field(
        &self,
        ghost: &str,
        columns: usize,
        theme: &Theme,
    ) -> (String, usize, Option<usize>) {
        if columns == 0 {
            return (String::new(), 0, None);
        }
        let (start, end) = (self.cursor.min(self.anchor), self.cursor.max(self.anchor));
        let ghost = if start == end && self.cursor == self.value.len() {
            render::clean(ghost)
        } else {
            String::new()
        };
        let ghost = if render::text_width(&self.value) + render::text_width(&ghost) < columns {
            ghost
        } else {
            String::new()
        };
        let mut first = 0;
        while first < self.cursor && render::text_width(&self.value[first..self.cursor]) >= columns
        {
            first += self.value[first..].chars().next().map_or(0, char::len_utf8);
        }
        let room = columns - 1;
        let (mut out, mut used, mut caret, mut ink) = (String::new(), 0, None, "");
        let (mut shown, mut boundary) = (String::new(), None);
        for (offset, c) in self.value[first..].char_indices() {
            let offset = first + offset;
            if offset == self.cursor {
                caret = Some(used);
                boundary = Some(shown.len());
            }
            if !render::safe(c) {
                continue;
            }
            let cells = super::cells::width(c);
            if used + cells > room {
                break;
            }
            let wanted = if offset >= start && offset < end {
                "stem"
            } else if !self.full_name && offset >= self.editable_end {
                "ext"
            } else {
                "text"
            };
            if wanted != ink {
                ink = wanted;
                out.push_str(&match ink {
                    "stem" if theme.depth == Depth::Sixteen => {
                        format!("\x1b[0m{}{}\x1b[7m", theme.field, theme.foreground)
                    }
                    "stem" => format!("\x1b[0m{}{}", theme.stem, theme.foreground),
                    "ext" => format!("\x1b[0m{}{}", theme.field, theme.ext),
                    _ => format!("\x1b[0m{}{}", theme.field, theme.foreground),
                });
            }
            out.push(c);
            shown.push(c);
            used += cells;
        }
        if self.cursor == self.value.len() && used <= room {
            caret = Some(used);
            boundary = Some(shown.len());
        }
        if super::cells::visual::needed(&shown) {
            caret = boundary.map(|at| super::cells::visual::caret(&shown, at));
            if let Some((open, close)) = super::cells::markers(super::cells::Run::Name) {
                out = format!("{open}{out}{close}");
            }
        }
        out.push_str(&format!("\x1b[0m{}{}{ghost} ", theme.field, theme.ext));
        used += render::text_width(&ghost) + 1;
        (out, used, caret.filter(|_| start == end))
    }
}

#[cfg(test)]
#[path = "editor_tests.rs"]
mod tests;
