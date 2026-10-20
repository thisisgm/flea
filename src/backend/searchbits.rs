// Letter presence is a rejection filter only; unknown bytes always keep the candidate.
pub(crate) const UNKNOWN: u64 = 1 << 63;
const LITERALS: u64 = ((1 << 16) - 1) << 26;

pub(crate) fn mask(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0, |bits, byte| bits | match byte.to_ascii_lowercase() {
        b'a'..=b'z' => 1 << (byte.to_ascii_lowercase() - b'a'),
        b'0'..=b'9' => 1 << (26 + byte - b'0'),
        b'/' => 1 << 36,
        b'-' => 1 << 37,
        b'_' => 1 << 38,
        b'.' => 1 << 39,
        b' ' => 1 << 40,
        b'+' => 1 << 41,
        _ => UNKNOWN,
    })
}

pub(crate) fn possible(query: u64, candidate: u64, edits: usize) -> bool {
    if (query | candidate) & UNKNOWN != 0 { return true; }
    let missing = query & !candidate;
    missing & LITERALS == 0 && missing.count_ones() as usize <= edits
}
