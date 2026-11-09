//! A non-UTF-8 component travels as `<lossy>\0<raw hex>`, which no real file is named and the OS refuses unconverted (docs/architecture/exact-names.md).
use std::borrow::Cow;
use std::ffi::OsStr;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

const MARK: u8 = 0;
const HEX: &[u8; 16] = b"0123456789abcdef";

// Sample input: b"col\xff" answers "col\u{fffd}\u{0}636f6cff"; b"plain.txt" answers itself, borrowed.
pub fn name_text(raw: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(raw) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => {
            let mut out = String::from_utf8_lossy(raw).into_owned();
            push_exact(&mut out, raw);
            Cow::Owned(out)
        }
    }
}

// The text of a whole path, each component as name_text writes it; replaces to_string_lossy wherever the text names a file.
pub fn path_text(path: &Path) -> Cow<'_, str> {
    let raw = path.as_os_str().as_bytes();
    if let Ok(text) = std::str::from_utf8(raw) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(raw.len() * 3);
    for (at, part) in raw.split(|&b| b == b'/').enumerate() {
        if at > 0 {
            out.push('/');
        }
        out.push_str(&name_text(part));
    }
    Cow::Owned(out)
}

// The exact path a text names. A component whose tail is not the canonical spelling keeps its NUL, so the OS refuses it.
// Sample input: "/d/col\u{fffd}\u{0}636f6cff" answers the path /d/col<0xff>; "/d/a.txt" answers itself, borrowed.
pub fn text_path(text: &str) -> Cow<'_, Path> {
    if !text.as_bytes().contains(&MARK) {
        return Cow::Borrowed(Path::new(text));
    }
    let mut out = Vec::with_capacity(text.len());
    for (at, part) in text.as_bytes().split(|&b| b == b'/').enumerate() {
        if at > 0 {
            out.push(b'/');
        }
        match exact_of(part) {
            Some(raw) => out.extend_from_slice(&raw),
            None => out.extend_from_slice(part),
        }
    }
    Cow::Owned(PathBuf::from(std::ffi::OsString::from_vec(out)))
}

// One name's exact bytes; the text a listing row carries goes through here before any join.
pub fn text_name(text: &str) -> Cow<'_, OsStr> {
    match text_path(text) {
        Cow::Borrowed(path) => Cow::Borrowed(path.as_os_str()),
        Cow::Owned(path) => Cow::Owned(path.into_os_string()),
    }
}

// A child process's arguments: argv carries no NUL, so each exact text goes out as the bytes it names.
// Sample input: ["--", "/d/col\u{fffd}\u{0}636f6cff"] answers "--" and the path /d/col<0xff>.
pub fn args<'a, S: AsRef<str> + ?Sized + 'a>(
    texts: impl IntoIterator<Item = &'a S>,
) -> impl Iterator<Item = Cow<'a, OsStr>> {
    texts.into_iter().map(|text| text_name(text.as_ref()))
}

// What a person reads: the lossy spelling alone, with every exact tail dropped.
// Sample input: "/d/col\u{fffd}\u{0}636f6cff/x" answers "/d/col\u{fffd}/x"; text with no NUL answers itself, borrowed.
pub fn display(text: &str) -> Cow<'_, str> {
    if !text.as_bytes().contains(&MARK) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut hidden = false;
    for c in text.chars() {
        match c {
            '\0' => hidden = true,
            '/' => {
                hidden = false;
                out.push('/');
            }
            c if !hidden => out.push(c),
            _ => {}
        }
    }
    Cow::Owned(out)
}

// A path from text or from bytes: text is decoded, a Path or OsStr is already exact.
pub trait ExactPath {
    fn exact_view(&self) -> Cow<'_, Path>;
}

impl ExactPath for str {
    fn exact_view(&self) -> Cow<'_, Path> {
        text_path(self)
    }
}

impl ExactPath for String {
    fn exact_view(&self) -> Cow<'_, Path> {
        text_path(self)
    }
}

impl ExactPath for Cow<'_, str> {
    fn exact_view(&self) -> Cow<'_, Path> {
        text_path(self)
    }
}

impl ExactPath for Path {
    fn exact_view(&self) -> Cow<'_, Path> {
        Cow::Borrowed(self)
    }
}

impl ExactPath for PathBuf {
    fn exact_view(&self) -> Cow<'_, Path> {
        Cow::Borrowed(self)
    }
}

impl ExactPath for OsStr {
    fn exact_view(&self) -> Cow<'_, Path> {
        Cow::Borrowed(Path::new(self))
    }
}

impl ExactPath for std::ffi::OsString {
    fn exact_view(&self) -> Cow<'_, Path> {
        Cow::Borrowed(Path::new(self))
    }
}

impl<T: ExactPath + ?Sized> ExactPath for &T {
    fn exact_view(&self) -> Cow<'_, Path> {
        (**self).exact_view()
    }
}

// Stands in for Path::new on anything that may carry wire text.
pub fn view<T: ExactPath + ?Sized>(from: &T) -> Cow<'_, Path> {
    from.exact_view()
}

// Stands in for PathBuf::from on anything that may carry wire text.
pub fn exact<T: ExactPath>(from: T) -> PathBuf {
    from.exact_view().into_owned()
}

// Stands in for to_string_lossy wherever the text names a file: exact for a name that is not UTF-8, identical otherwise.
pub trait ExactText {
    fn exact_text(&self) -> Cow<'_, str>;
}

impl ExactText for Path {
    fn exact_text(&self) -> Cow<'_, str> {
        path_text(self)
    }
}

impl ExactText for OsStr {
    fn exact_text(&self) -> Cow<'_, str> {
        path_text(Path::new(self))
    }
}

// True when the text names no path: a NUL survives decoding, as a forged tail or a stray NUL leaves.
pub fn is_refused(text: &str) -> bool {
    text_path(text).as_os_str().as_bytes().contains(&MARK)
}

// True when the text carries a name that is not UTF-8.
pub fn is_exact(text: &str) -> bool {
    text.as_bytes().contains(&MARK)
}

fn push_exact(out: &mut String, raw: &[u8]) {
    out.push('\0');
    for &b in raw {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 15) as usize] as char);
    }
}

// Only the one spelling name_text writes decodes: a forged tail, valid UTF-8 or a mismatched prefix stays refused.
// Sample input: b"col\xef\xbf\xbd\x00636f6cff" answers Some(b"col\xff"); b"a\x00zz" answers None.
fn exact_of(part: &[u8]) -> Option<Vec<u8>> {
    let at = part.iter().position(|&b| b == MARK)?;
    let (shown, tail) = (&part[..at], &part[at + 1..]);
    if tail.is_empty() || tail.len() % 2 != 0 {
        return None;
    }
    let mut raw = Vec::with_capacity(tail.len() / 2);
    for pair in tail.chunks(2) {
        raw.push(nibble(pair[0])? << 4 | nibble(pair[1])?);
    }
    if std::str::from_utf8(&raw).is_ok() || raw.contains(&b'/') || raw.contains(&MARK) {
        return None;
    }
    (String::from_utf8_lossy(&raw).as_bytes() == shown).then_some(raw)
}

fn nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(bytes: &[u8]) -> PathBuf {
        PathBuf::from(OsStr::from_bytes(bytes))
    }

    #[test]
    fn a_utf8_name_is_its_own_text_and_borrows() {
        for name in ["a.txt", "col\u{fffd}", "two\nlines", "caf\u{e9}", ""] {
            assert!(
                matches!(name_text(name.as_bytes()), Cow::Borrowed(text) if text == name),
                "{name:?}"
            );
            assert!(
                matches!(text_path(name), Cow::Borrowed(path) if path == Path::new(name)),
                "{name:?}"
            );
            assert!(
                matches!(display(name), Cow::Borrowed(text) if text == name),
                "{name:?}"
            );
        }
    }

    #[test]
    fn a_name_that_is_not_utf8_round_trips_and_never_meets_its_lossy_twin() {
        let text = name_text(b"col\xff");
        assert_eq!(text, "col\u{fffd}\u{0}636f6cff");
        assert_ne!(
            text, "col\u{fffd}",
            "the invalid name and its lossy twin must stay two texts"
        );
        assert_eq!(text_path(&text).as_os_str().as_bytes(), b"col\xff");
        assert_eq!(display(&text), "col\u{fffd}");
        assert!(is_exact(&text) && !is_exact("col\u{fffd}"));
    }

    #[test]
    fn a_path_converts_component_by_component() {
        let path = raw(b"/home/u/d\xe9j\xe0/sub/f\xfe.txt");
        let text = path_text(&path);
        assert_eq!(
            text,
            "/home/u/d\u{fffd}j\u{fffd}\u{0}64e96ae0/sub/f\u{fffd}.txt\u{0}66fe2e747874"
        );
        assert_eq!(text_path(&text).as_ref(), path.as_path());
        assert_eq!(
            display(&text),
            "/home/u/d\u{fffd}j\u{fffd}/sub/f\u{fffd}.txt"
        );
        assert_eq!(
            text_path(&format!("{text}/inner.txt")).as_ref(),
            path.join("inner.txt").as_path()
        );
    }

    #[test]
    fn a_forged_or_damaged_tail_keeps_its_nul_so_the_os_refuses_it() {
        for forged in [
            "a\0",
            "a\u{0}f",
            "a\0zz",
            "a\u{fffd}\0FF",
            "ab\u{0}61",
            "x\u{fffd}\u{0}ff",
            "col\u{fffd}\u{0}636f6cff00",
            "a\u{fffd}\u{0}2fff",
        ] {
            let path = text_path(forged);
            assert!(
                path.as_os_str().as_bytes().contains(&0),
                "{forged:?} must stay refused"
            );
            assert!(std::fs::metadata(&path).is_err());
        }
    }

    #[test]
    fn a_path_guard_takes_exact_text_and_refuses_a_surviving_nul() {
        assert!(!is_refused(&path_text(&raw(b"/d/col\xff"))));
        assert!(!is_refused("/d/plain.txt"));
        for forged in ["/d/a\0", "/d/a\u{fffd}\u{0}FF", "/d/x\u{fffd}\u{0}ff"] {
            assert!(is_refused(forged), "{forged:?}");
        }
    }

    #[test]
    fn a_child_argument_carries_the_exact_bytes_and_never_a_nul() {
        let text = path_text(&raw(b"/d/col\xff")).into_owned();
        let argv = [
            "--",
            "--file-name=/d/col\u{fffd}\u{0}636f6cff",
            text.as_str(),
        ];
        let out: Vec<Vec<u8>> = args(&argv).map(|arg| arg.as_bytes().to_vec()).collect();
        assert_eq!(
            out,
            [
                b"--".to_vec(),
                b"--file-name=/d/col\xff".to_vec(),
                b"/d/col\xff".to_vec()
            ]
        );
        let spoken = std::process::Command::new("printf")
            .arg("%s")
            .args(args(&[text.as_str()]))
            .output()
            .unwrap();
        assert_eq!(
            spoken.stdout, b"/d/col\xff",
            "the child reads the name's own bytes"
        );
    }
}
