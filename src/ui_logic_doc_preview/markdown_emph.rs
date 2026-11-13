//! CommonMark delimiter runs, resolved after the inline scan: a mark that pairs becomes a tag, any other stays text.

use super::markdown_escape::{escape_text, is_ascii_punct};
use super::markdown_piece::Delim;
use super::markdown_text::{at, char_at, char_before, is_space};
use std::collections::HashMap;

const RULE_OF_THREE: usize = 3;
const STRONG_LENGTH: usize = 2;
const MAX_RUN_LENGTH: usize = 1024;

// Unicode punctuation and symbols, approximated by the blocks that hold them.
fn is_unicode_punct(c: char) -> bool {
    matches!(c as u32,
        0xa1..=0xa9 | 0xab..=0xb1 | 0xb4 | 0xb6..=0xb8 | 0xbb | 0xbf | 0xd7 | 0xf7
            | 0x2c2..=0x2c5 | 0x2d2..=0x2df | 0x2010..=0x2027 | 0x2030..=0x205e
            | 0x20a0..=0x20c0 | 0x2100..=0x214f | 0x2190..=0x2bff | 0x2e00..=0x2e7f
            | 0x3001..=0x3003 | 0x3008..=0x3020 | 0x3030 | 0x30fb | 0xfe10..=0xfe19
            | 0xfe30..=0xfe6b | 0xff01..=0xff0f | 0xff1a..=0xff20 | 0xff3b..=0xff40
            | 0xff5b..=0xff65 | 0xffe0..=0xffee)
}

fn is_punct_char(c: char) -> bool {
    is_ascii_punct(c as u32) || is_unicode_punct(c)
}

// The scan reads the code unit before a run, so a character past the BMP before it reads as a letter.
fn punct_before(c: char) -> bool {
    (c as u32) <= 0xffff && is_punct_char(c)
}

// The scan reads the code unit after a run, so a pictograph past the BMP after it reads as punctuation.
fn punct_after(c: char) -> bool {
    if (c as u32) > 0xffff {
        (0x1f000..=0x1fbff).contains(&(c as u32))
    } else {
        is_punct_char(c)
    }
}

/// The delimiter run at body[i]: "foo*bar*" at 3 reads one star flanked by letters, which can open and close.
pub fn run_at(body: &str, i: usize, at_piece: usize) -> Delim {
    let ch = at(body, i);
    let mut j = i + 1;
    while j < body.len() && j - i < MAX_RUN_LENGTH && at(body, j) == ch {
        j += 1;
    }
    let before = char_before(body, i).unwrap_or('\n');
    let after = char_at(body, j).unwrap_or('\n');
    let space_before = is_space(before);
    let space_after = is_space(after);
    let punct_before = punct_before(before);
    let punct_after = punct_after(after);
    let left = !space_after && (!punct_after || space_before || punct_before);
    let right = !space_before && (!punct_before || space_after || punct_after);
    let under = ch == b'_';
    Delim {
        ch,
        n: j - i,
        orig: j - i,
        at: at_piece,
        end: j,
        open: if under {
            left && (!right || punct_before)
        } else {
            left
        },
        close: if under {
            right && (!left || punct_after)
        } else {
            right
        },
        before: String::new(),
        after: String::new(),
        opens: 0,
        closes: 0,
    }
}

/// Pairs the delimiters from index from on as the reference algorithm does, over a linked list so a pair never rescans its inside.
pub fn process(delims: &mut [Delim], from: usize) {
    let total = delims.len();
    if from >= total {
        return;
    }
    // Links run from-1 ..= total; index k is stored at k + 1 so from - 1 fits.
    let mut prev: Vec<isize> = vec![0; total + 2];
    let mut next: Vec<isize> = vec![0; total + 2];
    for k in from..total {
        prev[k + 1] = k as isize - 1;
        next[k + 1] = k as isize + 1;
    }
    let mut bottoms: HashMap<(u8, usize, bool), isize> = HashMap::new();
    let bottom_default = from as isize - 1;
    let mut ci = from as isize;
    while (ci as usize) < total {
        let c = ci as usize;
        if !delims[c].close || delims[c].n == 0 {
            ci = next[c + 1];
            continue;
        }
        let key = (delims[c].ch, delims[c].orig % RULE_OF_THREE, delims[c].open);
        let bottom = *bottoms.get(&key).unwrap_or(&bottom_default);
        let mut oi = prev[c + 1];
        while oi > bottom {
            let cand = &delims[oi as usize];
            let closer = &delims[c];
            if cand.open
                && cand.n > 0
                && cand.ch == closer.ch
                && !((closer.open || cand.close)
                    && (cand.orig + closer.orig).is_multiple_of(RULE_OF_THREE)
                    && !(cand.orig.is_multiple_of(RULE_OF_THREE)
                        && closer.orig.is_multiple_of(RULE_OF_THREE)))
            {
                break;
            }
            oi = prev[oi as usize + 1];
        }
        if oi <= bottom {
            bottoms.insert(key, prev[c + 1]);
            ci = next[c + 1];
            continue;
        }
        let o = oi as usize;
        let strong = delims[c].n >= STRONG_LENGTH && delims[o].n >= STRONG_LENGTH;
        let used = if strong { STRONG_LENGTH } else { 1 };
        delims[o].n -= used;
        delims[c].n -= used;
        let (open_tag, close_tag) = if strong {
            ("<strong>", "</strong>")
        } else {
            ("<em>", "</em>")
        };
        delims[o].after.insert_str(0, open_tag);
        delims[c].before.push_str(close_tag);
        delims[o].opens += 1;
        delims[c].closes += 1;
        // Everything between the pair can no longer pair: unlink it, and the opener too once it has no marks left.
        next[o + 1] = ci;
        prev[c + 1] = oi;
        if delims[o].n == 0 {
            let before = prev[o + 1];
            next[(before + 1) as usize] = ci;
            prev[c + 1] = before;
        }
        if delims[c].n == 0 {
            ci = next[c + 1];
        }
    }
}

/// The marks of a run left unpaired: escaped when the run could open or close, so no renderer pairs them again.
pub fn literal(d: &Delim) -> String {
    if d.n == 0 {
        return String::new();
    }
    let mark = (d.ch as char).to_string();
    if d.open || d.close {
        escape_text(&mark).repeat(d.n)
    } else {
        mark.repeat(d.n)
    }
}
