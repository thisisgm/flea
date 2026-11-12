//! A double-click's word, shared by every text field that edits.
use super::{Entry, Kind};
use crate::path::field::Line;
use gpui::Pixels;
use unicode_segmentation::UnicodeSegmentation;

impl Entry {
    /// Linux single-line word motion: previous word start, next word end; vertical Control arrows reach the line edges.
    pub fn control_move(&mut self, key: &str, held: gpui::Modifiers) -> bool {
        if !held.control
            || held.alt
            || held.platform
            || held.function
            || self.kind == Kind::Private
            || self.marked.is_some()
        {
            return false;
        }
        let at = self.cursor();
        let to = match key {
            "left" if !held.shift && !self.selected.is_empty() => self.selected.start,
            "right" if !held.shift && !self.selected.is_empty() => self.selected.end,
            "left" => self
                .text
                .unicode_word_indices()
                .map(|(start, _)| start)
                .take_while(|&start| start < at)
                .last()
                .unwrap_or(0),
            "right" => self
                .text
                .unicode_word_indices()
                .map(|(start, word)| start + word.len())
                .find(|&end| end > at)
                .unwrap_or(self.text.len()),
            "up" => 0,
            "down" => self.text.len(),
            _ => return false,
        };
        if held.shift {
            self.select_to(to);
        } else {
            self.move_to(to);
        }
        true
    }

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

    #[test]
    fn fx466_control_motion_excludes_passwords_composition_and_other_modifiers() {
        let control = gpui::Modifiers {
            control: true,
            ..Default::default()
        };
        for held in [
            gpui::Modifiers::default(),
            gpui::Modifiers {
                alt: true,
                ..control
            },
            gpui::Modifiers {
                platform: true,
                ..control
            },
            gpui::Modifiers {
                function: true,
                ..control
            },
        ] {
            let mut field = entry("alpha beta", Kind::Text);
            field.move_to(field.text.len());
            let before = super::super::navigation_cases::snapshot(&field);
            assert!(!field.control_move("left", held));
            assert_eq!(super::super::navigation_cases::snapshot(&field), before);
        }
        let mut private = entry("alpha beta", Kind::Private);
        assert!(!private.control_move("left", control));
        assert_eq!(private.selected, 0..10);
        let mut field = entry("alpha beta", Kind::Text);
        field.replace_and_mark(None, "にほん", None);
        let before = super::super::navigation_cases::snapshot(&field);
        for key in ["left", "right", "up", "down"] {
            assert!(!field.control_move(key, control));
            assert_eq!(super::super::navigation_cases::snapshot(&field), before);
        }
    }

    #[test]
    fn fx466_word_targets_never_split_a_grapheme_even_from_an_interior_caret() {
        use unicode_segmentation::UnicodeSegmentation;
        for text in [
            "e\u{301}cole café",
            "hi \u{1f469}\u{200d}\u{1f4bb} \u{1f1e8}\u{1f1e6} name",
            "हिन्दी 日本語",
        ] {
            for at in text.char_indices().map(|(at, _)| at) {
                for key in ["left", "right", "up", "down"] {
                    let mut field = entry(text, Kind::Text);
                    field.move_to(at);
                    assert!(field.control_move(
                        key,
                        gpui::Modifiers {
                            control: true,
                            ..Default::default()
                        }
                    ));
                    assert!(
                        field.cursor() == text.len()
                            || text
                                .grapheme_indices(true)
                                .any(|(at, _)| at == field.cursor()),
                        "{text:?}: {key} from {at}"
                    );
                }
            }
        }
    }

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
