// ASCII candidates expose folded letters without allocating or expanding each byte to a Letter.
use super::{fold, Letter, Matcher, State, EXACT, NAME, PENDING_NAME, PREFIX, TYPO, WORD};

pub(crate) trait Text: Copy {
    fn len(self) -> usize;
    fn lower(self, at: usize) -> char;
    fn boundary(self, at: usize) -> bool;
    fn alphabetic(self, at: usize) -> bool { self.lower(at).is_alphabetic() }
}

impl Text for &[Letter] {
    fn len(self) -> usize { self.len() }
    fn lower(self, at: usize) -> char { self[at].lower }
    fn boundary(self, at: usize) -> bool { self[at].boundary }
}

#[derive(Clone, Copy)]
pub(crate) struct Ascii<'a>(pub &'a [u8]);

impl Text for Ascii<'_> {
    fn len(self) -> usize { self.0.len() }
    fn lower(self, at: usize) -> char { self.0[at].to_ascii_lowercase() as char }
    fn boundary(self, at: usize) -> bool {
        at == 0 || super::separator(self.0[at - 1] as char)
            || (self.0[at].is_ascii_uppercase() && !self.0[at - 1].is_ascii_uppercase())
    }
    fn alphabetic(self, at: usize) -> bool { self.0[at].is_ascii_alphabetic() }
}

pub(super) fn name(matcher: &Matcher, query: &[u8], bytes: &[u8], letters: &mut Vec<Letter>, state: &mut State, edits: &mut crate::backend::searchtypo::Scratch) -> Option<i32> {
    if query.is_empty() { return Some(0); }
    let hay = Ascii(bytes);
    let prefix = |at| bytes.get(at..at + query.len()).is_some_and(|part| part.eq_ignore_ascii_case(query));
    if prefix(0) { return Some(if bytes.len() == query.len() { EXACT } else { PREFIX }); }
    let mut matched = 0;
    for at in 0..bytes.len() {
        if hay.lower(at) as u8 == query[0] && at > 0 && hay.boundary(at) && prefix(at) { return Some(WORD); }
        if query.get(matched).copied() == Some(hay.lower(at) as u8) { matched += 1; }
    }
    if matched == query.len() {
        if matcher.defer { return Some(PENDING_NAME); }
        fold(bytes, letters);
        state.reset();
        state.advance(&matcher.needle, letters, true);
        return state.score(NAME);
    }
    let distance = match matcher.typo.ascii_distance(hay, matcher.budget) {
        Some(distance) => distance,
        None => {
            fold(bytes, letters);
            matcher.distance(letters, edits)
        }
    };
    distance.map(|distance| TYPO - distance as i32 * 1000)
}
