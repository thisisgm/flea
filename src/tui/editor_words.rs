//! Yazi input/actor/{backward,forward,kill}.rs, with UTF-8 byte positions.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Space,
    Punct,
    Word,
}
fn kind(c: char) -> Kind {
    if c.is_whitespace() {
        Kind::Space
    } else if (c.is_ascii_punctuation() && c != '_') || matches!(c, '！' | '，' | '；' | '？') {
        Kind::Punct
    } else {
        Kind::Word
    }
}
pub fn backward(value: &str, cursor: usize) -> usize {
    let mut chars = value[..cursor].char_indices().rev();
    let Some((_, c)) = chars.next() else {
        return 0;
    };
    let mut previous = kind(c);
    for (at, c) in chars {
        let current = kind(c);
        if previous == Kind::Word && current != Kind::Word {
            return at + c.len_utf8();
        }
        previous = current;
    }
    0
}
pub fn forward(value: &str, cursor: usize, end: usize) -> usize {
    let mut chars = value[cursor..end].char_indices();
    let Some((_, c)) = chars.next() else {
        return end;
    };
    let mut previous = kind(c);
    for (index, (at, c)) in chars.enumerate() {
        let current = kind(c);
        if previous == Kind::Word && current != Kind::Word && index != 0 {
            return cursor + at;
        }
        previous = current;
    }
    end
}
pub fn kill_length(chars: impl Iterator<Item = char>) -> usize {
    let mut class = Kind::Space;
    let mut bytes = 0;
    for c in chars {
        let current = kind(c);
        if class != Kind::Space && class != current {
            break;
        }
        class = current;
        bytes += c.len_utf8();
    }
    bytes
}
