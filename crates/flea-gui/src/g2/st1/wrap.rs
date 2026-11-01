//! The pieces of a Settings label its room keeps whole, so a word never breaks where the room could have held it.

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
