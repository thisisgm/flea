// A read-only look at a directory that is not the one the pane is on. The columns view needs the
// rows of every ancestor at once, and `list` can only ever hold one listing, so this answers
// without touching it.
use crate::backend::icons::Names;
use crate::backend::mime::Db;
use crate::backend::scan::{mode_of, scan};
use crate::backend::sort::sort_by_name;
use crate::json::escape;

// A column draws what fits on screen, so a peek at a huge directory is capped rather than streamed.
pub const PEEK_CAP: usize = 512;

// Phase 1 only: a column row draws a mark and a name, so no row here is ever stat'd. That is what
// makes a peek cheap enough to run once per ancestor on every directory change.
// corner: a symlink therefore draws its own mark rather than its target's, unlike a listing row.
pub fn peek_line(path: &str, first: usize, hidden: bool, mime: &Db, icons: &Names) -> String {
    let (mut listing, _read_ms) = match scan(path, hidden) {
        Ok(v) => v,
        // An unreadable ancestor is still not an error for the pane it belongs to, but it is not an
        // empty directory either, and the column drew those two the same way until this line.
        Err(_) => return failed_peek(path, hidden),
    };
    sort_by_name(&mut listing, false);
    let total = listing.len();
    let count = first.min(PEEK_CAP).min(total);
    let mut out = String::with_capacity(count * 64);
    // Echoed so the columns view and the path bar's Tab tell their two replies apart; both ask with
    // pane.windowSize, so first cannot differ between them and path plus hidden is the whole key.
    out.push_str(&format!(
        r#"{{"t":"peeked","path":"{}","hidden":{},"n":{},"rows":["#,
        escape(path), hidden, total
    ));
    for i in 0..count {
        if i > 0 {
            out.push(',');
        }
        let name = listing.name(i);
        let dir = listing.is_dir(i);
        // Mode 0, because no row here is stat'd; the execute-bit fallback for an application type
        // therefore resolves to the generic mark rather than the executable one in a column.
        let icon = icons.icon_for(mime.lookup(name), dir, 0);
        out.push_str(&format!(
            r#"{{"n":"{}","d":{},"i":"{}"}}"#,
            escape(name), dir, escape(icon)
        ));
    }
    out.push_str("]}");
    out
}

// A scan that failed answers zero rows, which is the exact count an empty directory answers, so the
// line says which of the two it is. mode carries the directory's own permissions when the stat
// outlived the refused read, and is left out when it did not, the same rule the error line follows.
fn failed_peek(path: &str, hidden: bool) -> String {
    let mode = mode_of(path);
    let mode_field = if mode == 0 { String::new() } else { format!(r#","mode":{}"#, mode) };
    format!(
        r#"{{"t":"peeked","path":"{}","hidden":{},"n":0,"failed":true{},"rows":[]}}"#,
        escape(path), hidden, mode_field
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;

    fn tables() -> (Db, Names) {
        (Db::load(), Names::load())
    }

    #[test]
    fn a_peek_answers_names_marks_and_the_directory_bit_with_directories_first() {
        let d = TestDir::new("peek");
        d.dir("sub");
        d.file("a.txt", "body");
        d.file("b.png", "");
        let (mime, icons) = tables();
        let line = peek_line(&d.path().to_string_lossy(), 10, false, &mime, &icons);
        assert!(line.starts_with(r#"{"t":"peeked""#));
        // The flag the request carried, so an asker can tell its own reply from the other's.
        assert!(line.contains(r#""hidden":false"#), "the reply says what it was asked for: {}", line);
        // The marker file the sandbox carries is a dotfile, so hidden false drops it.
        assert!(line.contains(r#""n":4"#) || line.contains(r#""n":3"#), "got {}", line);
        let sub = line.find(r#""n":"sub""#).expect("the directory row");
        let txt = line.find(r#""n":"a.txt""#).expect("the file row");
        assert!(sub < txt, "directories sort first, the same as a listing");
        assert!(line.contains(r#""n":"sub","d":true,"i":"folder""#));
        assert!(line.contains(r#""n":"a.txt","d":false"#));
    }

    #[test]
    fn hidden_follows_the_same_rule_a_listing_uses() {
        let d = TestDir::new("peekhidden");
        d.file("visible.txt", "");
        d.file(".secret", "");
        let (mime, icons) = tables();
        let shown = peek_line(&d.path().to_string_lossy(), 10, false, &mime, &icons);
        assert!(!shown.contains(".secret"));
        assert!(shown.contains(r#""hidden":false"#));
        let all = peek_line(&d.path().to_string_lossy(), 10, true, &mime, &icons);
        assert!(all.contains(".secret"));
        assert!(all.contains(r#""hidden":true"#), "the two replies differ in the field that tells them apart");
    }

    #[test]
    fn a_directory_that_cannot_be_read_is_never_an_error_but_says_it_failed() {
        let d = TestDir::new("peekmissing");
        let (mime, icons) = tables();
        let line = peek_line(&d.join("never-existed").to_string_lossy(), 10, false, &mime, &icons);
        assert!(line.starts_with(r#"{"t":"peeked""#), "still an answer and not an error: {}", line);
        // A refusal is still a reply to one of the two askers, so it carries the flag too.
        assert!(line.contains(r#""hidden":false"#), "a refusal is still a reply to a request: {}", line);
        assert!(line.contains(r#""n":0"#));
        assert!(line.contains(r#""failed":true"#), "a column that could not look says so: {}", line);
        assert!(!line.contains(r#""mode":"#), "nothing to stat, so no mode is claimed: {}", line);
        assert!(line.ends_with(r#""rows":[]}"#));
    }

    #[test]
    fn an_empty_directory_and_an_unreadable_one_are_two_different_answers() {
        let empty = TestDir::new("peekempty");
        let locked = TestDir::new("peeklocked");
        let inner = locked.dir("noread");
        // Write and enter, never read: opendir is refused while stat still answers.
        std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o300)).unwrap();
        let (mime, icons) = tables();

        // The sandbox marker is a dotfile, so hidden false makes this directory look empty.
        let empty_line = peek_line(&empty.path().to_string_lossy(), 10, false, &mime, &icons);
        let locked_line = peek_line(&inner.to_string_lossy(), 10, false, &mime, &icons);

        assert!(empty_line.contains(r#""n":0"#), "got {}", empty_line);
        assert!(locked_line.contains(r#""n":0"#), "got {}", locked_line);
        assert!(!empty_line.contains(r#""failed""#), "an empty directory is not a failure: {}", empty_line);
        assert!(locked_line.contains(r#""failed":true"#), "got {}", locked_line);
        // 0o40300: the file-type bits plus write and execute, which is what the stat still answers.
        assert!(locked_line.contains(r#""mode":16576"#), "the mode the pane draws: {}", locked_line);

        std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn the_row_count_is_capped_while_the_total_still_tells_the_truth() {
        let d = TestDir::new("peekcap");
        for i in 0..12 {
            d.file(&format!("f{:03}.txt", i), "");
        }
        let (mime, icons) = tables();
        let line = peek_line(&d.path().to_string_lossy(), 5, false, &mime, &icons);
        assert!(line.contains(r#""n":12"#), "the total is the whole directory: {}", line);
        assert_eq!(line.matches(r#""d":"#).count(), 5, "only five rows were asked for");
    }

    #[test]
    fn a_name_needing_escapes_survives_the_hand_built_line() {
        let d = TestDir::new("peekescape");
        d.file(r#"say "hi".txt"#, "");
        let (mime, icons) = tables();
        let line = peek_line(&d.path().to_string_lossy(), 10, false, &mime, &icons);
        assert!(line.contains(r#"say \"hi\".txt"#), "got {}", line);
    }
}
