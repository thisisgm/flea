use super::{input::Key, render};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

pub struct Editor {
    pub kind: &'static str,
    pub value: String,
    pub path: PathBuf,
    pub pending: bool,
    pub error: String,
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
            error: String::new(),
            identity: None,
        }
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
    pub fn valid(&mut self) -> bool {
        if !self.full_name && self.editable_end == 0 {
            self.error = "Enter a filename before the extension.".into();
            return false;
        }
        if matches!(self.kind, "rename" | "mkdir" | "newfile")
            && !crate::backend::ops::valid_name(&self.value)
        {
            self.error = "Enter one filename without a path separator.".into();
            return false;
        }
        if self.kind == "rename" {
            match self.path.symlink_metadata() {
                Ok(meta)
                    if self.identity == Some((meta.dev(), meta.ino(), meta.mode() & 0o170000)) => {}
                _ => {
                    self.error = "Selected item changed; reopen Rename.".into();
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
                .map_or(0, char::len_utf8)).min(self.editable_end);
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
    }
    pub fn line(
        &self,
        prefix: &str,
        suffix: &str,
        columns: usize,
        base: &str,
        muted: &str,
        accent: &str,
    ) -> String {
        let prefix = render::fit(prefix, render::text_width(prefix).min(columns));
        let available = columns.saturating_sub(render::text_width(&prefix));
        let mut first = 0;
        while first < self.cursor
            && render::text_width(&self.value[first..self.cursor]) >= available.max(1)
        {
            first += self.value[first..].chars().next().map_or(0, char::len_utf8);
        }
        let mut out = if self.kind == "path" { format!("{accent}{prefix}{base}") } else { prefix };
        let mut used = 0;
        let (start, end) = (self.cursor.min(self.anchor), self.cursor.max(self.anchor));
        for (offset, c) in self.value[first..].char_indices() {
            let offset = first + offset;
            if offset == self.cursor && start == end && used < available {
                out.push_str(&format!("{accent}▏{base}"));
                used += 1;
            }
            let cells = render::text_width(&c.to_string());
            if used + cells > available {
                break;
            }
            if offset >= start && offset < end {
                out.push_str("\x1b[7m");
            } else if !self.full_name && offset >= self.editable_end {
                out.push_str(muted);
            }
            out.push(c);
            if offset >= start && offset < end {
                out.push_str(base);
            }
            used += cells;
        }
        if self.cursor == self.value.len() && start == end && used < available {
            out.push_str(&format!("{accent}▏{base}"));
            used += 1;
        }
        out.push_str(muted);
        let tail = if self.error.is_empty() {
            suffix.to_owned()
        } else {
            format!(" · {}", self.error)
        };
        out.push_str(&render::fit(&tail, available.saturating_sub(used)));
        out.push_str(base);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    #[test]
    fn path_prompt_and_caret_use_accent_without_coloring_typed_text() {
        let editor = Editor::new("path", "am".into(), "/".into());
        assert_eq!(editor.line(": ", "ber/", 12, "BASE", "MUTED", "ACCENT"),
            "ACCENT: BASEamACCENT▏BASEMUTEDber/   BASE");
    }
    #[test]
    fn rename_preserves_extension_unicode_and_changed_identity() {
        let sandbox = TestDir::new("tui-editor");
        let path = sandbox.file("café.txt", "original");
        let mut editor = Editor::rename("café.txt".into(), path.clone(), false).unwrap();
        editor.update(&Key::character('新', ""));
        assert_eq!(editor.value, "新.txt");
        editor.update(&Key::named("Left", ""));
        editor.update(&Key::named("Delete", ""));
        assert_eq!(editor.value, ".txt");
        editor.replace("../escape".into());
        assert!(!editor.valid());
        editor.replace("renamed.txt".into());
        assert!(editor.valid());
        let moved = sandbox.join("old");
        assert!(
            path.is_absolute()
                && path.starts_with(sandbox.path())
                && moved.starts_with(sandbox.path())
        );
        std::fs::rename(&path, moved).unwrap();
        sandbox.file("café.txt", "replacement");
        assert!(!editor.valid());
    }
}
