//! Relative Path in the Copy As flyout: each path is read from the folder the pane lists, one per line.
use super::build::{listing_entries, Context};
use super::copyas::{lines, relative, Format};
use super::rows::{copy_as_entries, flyout_leaves};
use super::testkit::check;

fn leaf_ids(leaves: &[super::entry::Entry]) -> String {
    leaves
        .iter()
        .map(|leaf| leaf.id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn a_relative_path_reads_from_the_listed_folder() {
    let base = "/home/gm/proj";
    check(
        "a row of the listing is its name",
        relative("/home/gm/proj/a.rs", base),
        "a.rs".into(),
    );
    check(
        "a search hit keeps the folders between it and the listing",
        relative("/home/gm/proj/src/ui/a.rs", base),
        "src/ui/a.rs".into(),
    );
    check(
        "a path outside the listing walks up",
        relative("/home/gm/other/a.txt", base),
        "../other/a.txt".into(),
    );
    check(
        "a sibling with the listing's name as a prefix still walks up",
        relative("/home/gm/project/a.txt", base),
        "../project/a.txt".into(),
    );
    check(
        "the listing itself is a dot",
        relative("/home/gm/proj/", base),
        ".".into(),
    );
    check(
        "a trailing slash on the listing changes nothing",
        relative("/home/gm/proj/a.rs", "/home/gm/proj/"),
        "a.rs".into(),
    );
    check(
        "from the root a path loses only its leading slash",
        relative("/etc/hosts", "/"),
        "etc/hosts".into(),
    );
    check(
        "the root seen from a folder walks all the way up",
        relative("/", "/home/gm"),
        "../..".into(),
    );
    check(
        "no listed folder leaves the path whole",
        relative("/home/gm/a.txt", ""),
        "/home/gm/a.txt".into(),
    );
}

#[test]
fn relative_path_covers_the_whole_selection_and_only_its_own_format_reads_the_folder() {
    check(
        "many selected items copy one per line",
        lines(
            &[
                "/home/gm/proj/a.rs",
                "/home/gm/proj/src/b.rs",
                "/home/gm/c.md",
            ],
            Format::Relative,
            "/home/gm/proj",
        ),
        "a.rs\nsrc/b.rs\n../c.md".into(),
    );
    check(
        "Path stays absolute beside a listed folder",
        lines(&["/home/gm/proj/a.rs"], Format::Path, "/home/gm/proj"),
        "/home/gm/proj/a.rs".into(),
    );
    check(
        "the leaf id and the format word name one format",
        (Format::parse("copyRelative"), Format::parse("relative")),
        (Some(Format::Relative), Some(Format::Relative)),
    );
}

#[test]
fn the_flyout_offers_relative_path_second_on_r_with_the_file_mark() {
    let leaves = copy_as_entries();
    check(
        "Relative Path follows Path",
        leaf_ids(&leaves),
        "copyPath,copyRelative,copyName,copyStem,copydirpath,copyUri,copyQuoted".into(),
    );
    let row = &leaves[1];
    check(
        "it wears the file mark and the r hint",
        (row.glyph, row.key_hint),
        (Some("file"), Some("r")),
    );
    let built = listing_entries(&Context::default());
    let copy = built
        .iter()
        .find(|entry| entry.id == "copyAs")
        .and_then(|entry| entry.submenu.clone())
        .unwrap();
    check(
        "a folder listing's menu carries it",
        leaf_ids(&copy),
        leaf_ids(&leaves),
    );
}

#[test]
fn a_history_listing_hides_relative_path_and_keeps_path() {
    let built = listing_entries(&Context {
        recent: true,
        ..Context::default()
    });
    let copy = built
        .iter()
        .find(|entry| entry.id == "copyAs")
        .and_then(|entry| entry.submenu.clone())
        .unwrap();
    check(
        "Recent has no folder of its own, so the row hides",
        leaf_ids(&copy),
        "copyPath,copyName,copyStem,copydirpath,copyUri,copyQuoted".into(),
    );
    check(
        "the flyout c opens alone hides it too",
        leaf_ids(&flyout_leaves("copyAs", true)),
        leaf_ids(&copy),
    );
    check(
        "and a folder listing's lone flyout keeps it",
        leaf_ids(&flyout_leaves("copyAs", false)),
        leaf_ids(&copy_as_entries()),
    );
}
