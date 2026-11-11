//! The pieces of a Settings label its room keeps whole, so a word never breaks where the room could have held it.
use super::{geometry::LABEL_LINES, view::SettingsView};
use gpui::{px, Font, FontWeight, TextRun, Window};

impl SettingsView {
    /// Values and captions yield enough room for the complete label within the existing two-line row.
    pub(super) fn label_width(&self, label: &str, window: &Window) -> f32 {
        let mut low = self.longest_word(label, window);
        let mut high = self
            .shaped(label.to_owned(), self.typeface.body, window)
            .max(low);
        let run = TextRun {
            len: label.len(),
            font: Font {
                weight: FontWeight::BOLD,
                ..self.typeface.font.clone()
            },
            color: Default::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        // Search only whole layout pixels; the room calculation adds its own rounding slack.
        while high.ceil() - low.floor() > 1. {
            let width = ((low + high) / 2.).floor();
            let fits = window
                .text_system()
                .shape_text(
                    label.to_owned().into(),
                    self.typeface.body,
                    std::slice::from_ref(&run),
                    Some(px(width)),
                    None,
                )
                .is_ok_and(|lines| {
                    lines
                        .iter()
                        .map(|line| line.wrap_boundaries().len() + 1)
                        .sum::<usize>()
                        <= LABEL_LINES
                });
            if fits {
                high = width;
            } else {
                low = width + 1.;
            }
        }
        high.ceil()
    }
}

// The marks GPUI's is_word_char keeps inside a word; any other mark is a place its wrap may break before.
const WORD_MARKS: &str =
    "-_.'$%@#^~,=:;!)]}\"\u{2019}\u{2018}\u{201d}\u{bb}\u{2026}\u{22ef}\u{2011}\u{202f}";

/// The label's words between plain spaces, cut where GPUI's wrap may break inside one, as opens_piece lists.
pub(super) fn unbroken(label: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut start = None;
    for (at, letter) in label.char_indices() {
        if letter == ' ' {
            pieces.extend(start.take().map(|from| &label[from..at]));
        } else if start.is_none() || opens_piece(letter) {
            pieces.extend(start.map(|from| &label[from..at]));
            start = Some(at);
        }
    }
    pieces.extend(start.map(|from| &label[from..]));
    pieces
}

/// Han, kana, CJK and fullwidth forms, or an ASCII, Latin-1 or general punctuation mark or space GPUI breaks before.
fn opens_piece(letter: char) -> bool {
    matches!(
        letter,
        '\u{3000}'..='\u{30FF}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{FF00}'..='\u{FFEF}'
    ) || ((letter.is_ascii_punctuation()
        || matches!(letter, '\u{A1}'..='\u{BF}' | '\u{2000}'..='\u{205E}'))
        && !WORD_MARKS.contains(letter))
}
