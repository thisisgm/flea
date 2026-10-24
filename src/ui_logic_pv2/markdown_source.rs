//! The Source view's chunks: whole lines about SOURCE_CHUNK_BYTES long, so a screenful of a large file costs a few chunks.

use super::markdown_text::{find_byte, rfind_byte};

/// The length a chunk of whole lines aims for.
pub const SOURCE_CHUNK_BYTES: usize = 4096;
/// No chunk is longer than this: a line that runs past it is cut at its last space past SOURCE_CHUNK_BYTES, or at the limit.
pub const SOURCE_CHUNK_MAX: usize = 2 * SOURCE_CHUNK_BYTES;

/// Where each chunk starts: 20000 bytes in lines of 100 answer [0, 4100, 8200, 12300]; a hard cut steps back off a split character.
pub fn source_chunk_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    let mut from = 0;
    while text.len() - from > SOURCE_CHUNK_MAX {
        let cut = find_byte(text, b'\n', from + SOURCE_CHUNK_BYTES)
            .filter(|cut| *cut <= from + SOURCE_CHUNK_MAX);
        let next = match cut {
            Some(cut) => cut + 1,
            None => match rfind_byte(text, b'\n', from + SOURCE_CHUNK_BYTES - 1)
                .filter(|back| *back > from)
            {
                Some(back) => back + 1,
                None => {
                    let space = rfind_byte(text, b' ', from + SOURCE_CHUNK_MAX - 1)
                        .filter(|space| *space >= from + SOURCE_CHUNK_BYTES);
                    match space {
                        Some(space) => space + 1,
                        None => {
                            let mut hard = from + SOURCE_CHUNK_MAX;
                            while !text.is_char_boundary(hard) {
                                hard -= 1;
                            }
                            hard
                        }
                    }
                }
            },
        };
        starts.push(next);
        from = next;
    }
    starts
}

/// Chunk i without the newline that ends it, if one does; a cut inside a line drops nothing, so the chunks are the text again.
pub fn source_chunk<'t>(text: &'t str, starts: &[usize], i: usize) -> &'t str {
    let mut end = starts.get(i + 1).copied().unwrap_or(text.len());
    if i + 1 < starts.len() && end > 0 && text.as_bytes()[end - 1] == b'\n' {
        end -= 1;
    }
    &text[starts[i]..end]
}

/// The line count the facts and the bar show: a trailing newline ends a line, only an unterminated tail adds one.
pub fn line_count(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    text.bytes().filter(|b| *b == b'\n').count() + usize::from(!text.ends_with('\n'))
}

#[cfg(test)]
#[path = "markdown_source_tests.rs"]
mod tests;
