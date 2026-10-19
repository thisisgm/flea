// A result row's name and folder from its wire path, and the typed run inside a name; see docs/protocol.md "search".

// Sample input: "tools/benches/bench-run.sh" draws "bench-run.sh".
pub fn base(path: &str) -> &str {
    path.rfind('/').map_or(path, |at| &path[at + 1..])
}

// The folder column relative to the scope, empty for a match in the scope itself.
pub fn location(path: &str) -> &str {
    path.rfind('/').map_or("", |at| &path[..at])
}

// Home's prefix reads as a tilde, the rule the chrome writes a path by.
pub fn tilde(path: &str, home: &str) -> String {
    if !home.is_empty() && (path == home || path.strip_prefix(home).is_some_and(|rest| rest.starts_with('/'))) {
        return format!("~{}", &path[home.len()..]);
    }
    path.to_owned()
}

// The byte range of the typed run inside a name, folding case one character at a time as the backend's match does.
pub fn run(hay: &str, needle: &str) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    for (start, _) in hay.char_indices() {
        let mut rest = hay[start..].char_indices();
        let mut end = start;
        let matched = needle.chars().all(|want| match rest.next() {
            Some((at, have)) if have.to_lowercase().eq(want.to_lowercase()) => {
                end = start + at + have.len_utf8();
                true
            }
            _ => false,
        });
        if matched {
            return Some((start, end));
        }
    }
    None
}

// One typed character: a single character at or above space that is not DEL.
pub fn is_printable(text: &str) -> bool {
    let mut chars = text.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if c >= ' ' && c != '\u{7f}')
}

// The clipboard's first line with every control character dropped, so a paste is one query.
pub fn clean_paste(text: &str) -> String {
    let line = text.split(['\r', '\n']).next().unwrap_or("");
    line.chars().filter(|c| *c >= ' ' && *c != '\u{7f}').collect()
}
