// tests/js/compare.js value checks: the title, the board's sentences and the patch rows.
use super::compare::*;
use super::meta::CompareState;

const PATCH: &str = "--- /w/notes-a.md\n+++ /w/notes-b.md\n@@ -9,12 +9,12 @@\n ## Previews\n \n Each kind asks for what it shows.\n\
-Thumbnails never load on phones.\n-The map holds every row.\n-A taller preview costs nothing.\n-Ports keep their slots.\n-Marks ship one size.\n-Facts fold into two lines.\n\
+Thumbnails stay cache-only on phones.\n+The map holds the visible rows.\n+A taller preview costs what a shorter one did.\n+Ports keep their slots while scrolling.\n+Marks share one optical size.\n+Facts fold into one line.\n \
Only the visible lines are asked for.\n A newer request cancels an older one.\n The frame runs to the column bottom.\n";

#[test]
fn titles_and_sentences() {
    let i18n = super::english();
    assert_eq!(title("a.md", "b.md", Some(1), &i18n), "a.md and b.md \u{b7} 1 line differs");
    assert_eq!(title("notes-a.md", "notes-b.md", Some(12), &i18n), "notes-a.md and notes-b.md \u{b7} 12 lines differ");
    assert_eq!(title("notes-a.md", "notes-b.md", None, &i18n), "notes-a.md and notes-b.md");
    // a file named like a placeholder is not read as one
    assert_eq!(title("%2", "b", Some(1), &i18n), "%2 and b \u{b7} 1 line differs");
    assert_eq!(sentence(&CompareState::Identical, &i18n), "The two files are identical.");
    assert_eq!(sentence(&CompareState::Binary, &i18n), "These files are binary, so there is nothing to compare.");
    // compare-large: the limit reads 1.0 MB, the decimal unit every other size on screen uses
    assert_eq!(sentence(&CompareState::Over, &i18n), "These files are over 1.0 MB, too large to compare.");
    assert_eq!(sentence(&CompareState::Failed, &i18n), "These files could not be compared.");
    assert_eq!(sentence(&CompareState::Diff("x".into()), &i18n), "");
}

#[test]
fn the_boards_patch_reads_as_rows() {
    let rows = rows(PATCH);
    assert_eq!((rows.len(), rows[0].kind, rows[0].text.as_str()), (19, Kind::Hunk, "@@ -9,12 +9,12 @@"));
    assert_eq!(changed(&rows), 12);
    assert_eq!((rows[1].kind, rows[1].old_no, rows[1].new_no, rows[1].text.as_str()), (Kind::Context, 9, 9, "## Previews"));
    assert_eq!((rows[4].kind, rows[4].old_no, rows[4].new_no), (Kind::Removed, 12, 0));
    assert_eq!((rows[10].kind, rows[10].old_no, rows[10].new_no, rows[10].text.as_str()), (Kind::Added, 0, 12, "Thumbnails stay cache-only on phones."));
    assert_eq!((rows[18].old_no, rows[18].new_no), (20, 20));
    // the blank context line keeps its place
    assert_eq!((rows[2].text.as_str(), rows[2].old_no), ("", 10));
}

#[test]
fn hunks_markers_and_notes() {
    let numbers: Vec<String> = rows("--- a\n+++ b\n@@ -1 +1 @@\n-x\n+y\n@@ -40,2 +40,2 @@\n z\n-q\n+r\n").iter()
        .map(|r| format!("{}/{}", r.old_no, r.new_no)).collect();
    assert_eq!(numbers.join(" "), "0/0 1/0 0/1 0/0 40/40 41/0 0/41");
    // a line holding --- inside a hunk is a removed line
    assert_eq!(rows("--- a\n+++ b\n@@ -1 +1 @@\n---x\n+y\n")[1].text, "--x");
    assert_eq!(rows("--- a\n+++ b\n@@ -1 +1 @@\n-x\n\\ No newline at end of file\n+y\n")[2].kind, Kind::Note);
    // nothing before the first hunk is a row, and a malformed header is not a hunk
    assert!(rows("--- a\n+++ b\n").is_empty());
    assert!(rows("@@ -x +1 @@\n-a\n").is_empty());
}

#[test]
fn which_selections_offer_compare() {
    let (reg, dir, link) = (0o100644, 0o040755, 0o120777);
    assert!(can_compare(&[reg, reg]));
    assert!(can_compare(&[0o100000, 0o100755]));
    assert!(!can_compare(&[reg]) && !can_compare(&[reg, reg, reg]));
    assert!(!can_compare(&[reg, dir]) && !can_compare(&[reg, 0o040000]) && !can_compare(&[reg, link]));
    assert!(!can_compare(&[]));
    assert_eq!((leaf("/w/notes-a.md"), leaf("b.txt"), leaf("/w/dir/")), ("notes-a.md", "b.txt", "dir"));
}
