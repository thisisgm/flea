// ASCII candidates expose folded letters without allocating or expanding each byte to a Letter.
use super::{fold, Letter, Matcher, State, BASENAME_BONUS, BOUNDARY_BONUS, EXACT, HIT_SPAN, NAME, PENDING_NAME, PREFIX, RUN_BONUS, TYPO, TYPO_EDIT, WORD};

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
        state.reset();
        matcher.advance(state, bytes, letters, true);
        return state.score(NAME);
    }
    typo(matcher, bytes, letters, edits)
}

// A name that holds no subsequence of the query scores only by its edit distance.
fn typo(matcher: &Matcher, bytes: &[u8], letters: &mut Vec<Letter>, edits: &mut crate::backend::searchtypo::Scratch) -> Option<i32> {
    let distance = match matcher.typo.ascii_distance(Ascii(bytes), matcher.budget) {
        Some(distance) => distance,
        None => {
            fold(bytes, letters);
            matcher.distance(letters, edits)
        }
    };
    distance.map(|distance| TYPO - distance as i32 * TYPO_EDIT)
}

// A name byte or a query letter is one bit of a u64 mask.
const MASK_BITS: usize = u64::BITS as usize;

// Per-letter position masks of one name and the DP columns over them, kept by each name worker.
pub(crate) struct Bits {
    masks: [u64; MASK_BITS],
    prev: [i32; MASK_BITS],
    cur: [i32; MASK_BITS],
    lift_prev: [i32; MASK_BITS],
    lift_cur: [i32; MASK_BITS],
}

impl Bits {
    pub(crate) fn new() -> Self {
        Self { masks: [0; MASK_BITS], prev: [0; MASK_BITS], cur: [0; MASK_BITS], lift_prev: [0; MASK_BITS], lift_cur: [0; MASK_BITS] }
    }
}

// A PENDING_NAME name's basename score and lift (the most it adds to a parent's DP column) as i16 within +- 1,200; one letter lifts NONE.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Own {
    hit: i16,
    lift: i16,
}

impl Own {
    pub(crate) const NONE: Own = Own { hit: i16::MIN, lift: i16::MIN };

    fn new(score: i32, lift: i32) -> Self {
        let hit = score - NAME;
        debug_assert!(hit.abs() < i32::from(i16::MAX) && (lift == NONE || lift.abs() < i32::from(i16::MAX)));
        Own { hit: hit as i16, lift: if lift == NONE { i16::MIN } else { lift as i16 } }
    }

    pub(crate) fn is_none(self) -> bool { self.hit == i16::MIN }

    pub(crate) fn score(self) -> i32 { NAME + i32::from(self.hit) }

    pub(crate) fn lift(self) -> i32 { if self.lift == i16::MIN { NONE } else { i32::from(self.lift) } }
}

// A dead DP cell, low enough that a gap or run added to it stays far under every live one.
const NONE: i32 = i32::MIN / 4;

// `name` for a name of at most MASK_BITS bytes, read once into a mask per query letter; with defer a subsequence gets its Own.
pub(super) fn name_own(matcher: &Matcher, lanes: &[u64; 256], bytes: &[u8], letters: &mut Vec<Letter>, edits: &mut crate::backend::searchtypo::Scratch,
    bits: &mut Bits) -> Option<(Option<i32>, Option<Own>)> {
    let m = matcher.needle.len();
    if m == 0 || bytes.len() > MASK_BITS { return None; }
    bits.masks[..m].fill(0);
    let mut starts = 0u64;
    for (at, &byte) in bytes.iter().enumerate() {
        let mut hits = lanes[byte as usize];
        if hits == 0 { continue; }
        let bit = 1u64 << at;
        if Ascii(bytes).boundary(at) { starts |= bit; }
        while hits != 0 {
            bits.masks[hits.trailing_zeros() as usize - 1] |= bit;
            hits &= hits - 1;
        }
    }
    Some(match scored(m, starts, bytes.len(), bits) {
        Some((score, lift)) if score < WORD && matcher.defer => (Some(PENDING_NAME), Some(Own::new(score, lift))),
        Some((score, _)) => (Some(score), None),
        None => (typo(matcher, bytes, letters, edits), None),
    })
}

// The run class `name` finds, else advance_lanes' basename DP and the lift DP (NONE for one letter); None for no subsequence.
fn scored(m: usize, starts: u64, len: usize, bits: &mut Bits) -> Option<(i32, i32)> {
    let Bits { masks, prev, cur, lift_prev, lift_cur } = bits;
    let masks = &masks[..m];
    let mut run = !0u64;
    for (k, mask) in masks.iter().enumerate() { run &= mask >> k; }
    if run & 1 != 0 { return Some((if len == masks.len() { EXACT } else { PREFIX }, NONE)); }
    if run & starts & !1 != 0 { return Some((WORD, NONE)); }
    let mut after = !0u64;
    for mask in masks {
        let open = mask & after;
        if open == 0 { return None; }
        after = (!0u64).checked_shl(open.trailing_zeros() + 1).unwrap_or(0);
    }
    let bonus = |at: u32| BASENAME_BONUS + if starts >> at & 1 != 0 { BOUNDARY_BONUS } else { 0 };
    let mut each = masks[0];
    while each != 0 {
        let at = each.trailing_zeros();
        (prev[at as usize], lift_prev[at as usize]) = (bonus(at), NONE);
        each &= each - 1;
    }
    for pair in masks.windows(2) {
        let (before, now) = (pair[0], pair[1]);
        // Earlier matches of the previous letter fold into `best` offset by their place; a lift chain may start here at 0.
        let (mut earlier, mut best, mut lift_best) = (before, NONE, 0);
        let mut each = now;
        while each != 0 {
            let at = each.trailing_zeros();
            while earlier != 0 && earlier.trailing_zeros() < at {
                let p = earlier.trailing_zeros();
                best = best.max(prev[p as usize] + p as i32 + 1);
                lift_best = lift_best.max(lift_prev[p as usize] + p as i32 + 1);
                earlier &= earlier - 1;
            }
            // A dead column stays under NONE / 2 through every gap and run, so one test keeps it dead.
            let ran = at > 0 && before >> (at - 1) & 1 != 0;
            let joined = if ran { prev[at as usize - 1] + RUN_BONUS } else { NONE };
            let from = (best - at as i32).max(joined);
            cur[at as usize] = if from <= NONE / 2 { NONE } else { from + bonus(at) };
            let lift_joined = if ran { lift_prev[at as usize - 1] + RUN_BONUS } else { NONE };
            lift_cur[at as usize] = (lift_best - at as i32).max(lift_joined) + bonus(at);
            each &= each - 1;
        }
        std::mem::swap(prev, cur);
        std::mem::swap(lift_prev, lift_cur);
    }
    let (mut hit, mut lift, mut each) = (NONE, NONE, masks[masks.len() - 1]);
    while each != 0 {
        let at = each.trailing_zeros() as usize;
        (hit, lift) = (hit.max(prev[at]), lift.max(lift_prev[at]));
        each &= each - 1;
    }
    // A subsequence always has a hit: the greedy chain above is one.
    debug_assert!(hit > NONE / 2);
    Some((NAME + hit.clamp(-HIT_SPAN, HIT_SPAN), if masks.len() == 1 { NONE } else { lift }))
}
