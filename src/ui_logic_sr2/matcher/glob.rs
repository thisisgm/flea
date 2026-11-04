//! The * and ? pattern test, the shortest stretch from the front of a name that satisfies it.

// A pattern matched against the front of hay, the shortest stretch that satisfies it; * takes any run and ? one char.
pub(super) fn glob_prefix(hay: &[char], pattern: &[char]) -> Option<usize> {
    let (mut h, mut p, mut star) = (0, 0, None::<(usize, usize)>);
    loop {
        super::super::steps(1);
        if p == pattern.len() {
            return Some(h);
        }
        if h < hay.len() && (pattern[p] == '?' || pattern[p] == hay[h]) {
            h += 1;
            p += 1;
        } else if pattern[p] == '*' {
            star = Some((p, h));
            p += 1;
        } else if let Some((at, from)) = star.filter(|(_, from)| *from < hay.len()) {
            star = Some((at, from + 1));
            p = at + 1;
            h = from + 1;
        } else {
            return None;
        }
    }
}
