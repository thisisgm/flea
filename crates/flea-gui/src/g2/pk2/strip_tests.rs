//! picker-header.qml strip cases onto crate::path::split.
use crate::path::split;

#[test]
fn the_strip_draws_parents_plus_leaf() {
    for path in ["/winprobe", "/a/b/winprobe", "/fx/v/work/notes.txt"] {
        let (parents, leaf) = split(path);
        assert_eq!(
            format!("{parents}{leaf}"),
            path,
            "the strip draws {path} whole"
        );
    }
}

#[test]
fn a_top_path_splits_into_slash_parents_and_its_leaf() {
    assert_eq!(split("/winprobe"), ("/", "winprobe"));
}

#[test]
fn a_deeper_path_keeps_the_directories_as_parents() {
    assert_eq!(split("/a/b/winprobe"), ("/a/b/", "winprobe"));
}
