//! A double-click's word, shared by every text field that edits.
use super::{Entry, Kind};
use crate::path::field::Line;
use gpui::Pixels;
use unicode_segmentation::UnicodeSegmentation;

impl Entry {
    /// Selects the word of the character at byte `at` as a text input does, the last one past the end; a password selects its whole line.
    pub fn select_word(&mut self, at: usize) {
        if self.kind == Kind::Private {
            self.select_all();
            return;
        }
        let at = at.min(self.text.len().saturating_sub(1));
        let word = self
            .text
            .split_word_bound_indices()
            .map(|(start, word)| start..start + word.len())
            .find(|word| word.contains(&at))
            .unwrap_or(at..at);
        self.move_to(word.start);
        self.select_to(word.end);
    }

    /// The byte of the glyph `line` drew under `x`, measured from the start of its shaped run, not the nearer caret edge.
    pub fn word_index(&self, line: &Line, x: Pixels) -> usize {
        let under = line.run.as_ref().and_then(|run| run.line().index_for_x(x));
        under.map_or(self.text.len(), |at| self.from_display(at))
    }

    /// select_word for the glyph `line` drew under `x`.
    pub fn select_word_under(&mut self, line: &Line, x: Pixels) {
        self.select_word(self.word_index(line, x));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str, kind: Kind) -> Entry {
        let mut entry = Entry {
            kind,
            ..Entry::default()
        };
        entry.begin(text);
        entry
    }

    fn word(text: &str, at: usize) -> String {
        let mut entry = entry(text, Kind::Text);
        entry.select_word(at);
        entry.text[entry.selected.clone()].to_owned()
    }

    #[test]
    fn a_word_is_the_run_around_the_character() {
        assert_eq!(word("alpha-beta", 2), "alpha");
        assert_eq!(word("alpha-beta", 5), "-");
        assert_eq!(word("alpha-beta", 7), "beta");
        assert_eq!(word("report final.pdf", 9), "final.pdf");
        assert_eq!(word("Résumé draft", 3), "Résumé");
    }

    #[test]
    fn a_press_past_the_end_takes_the_last_word_and_an_empty_line_nothing() {
        assert_eq!(word("alpha beta", 99), "beta");
        assert_eq!(word("", 0), "");
    }

    #[test]
    fn a_password_selects_its_whole_line_and_never_shows_a_boundary() {
        let mut entry = entry("alpha beta", Kind::Private);
        entry.select_word(2);
        assert_eq!(entry.selected, 0..10);
    }
}
