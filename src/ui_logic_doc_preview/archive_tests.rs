use super::*;

fn meta(entries: u64, unpacked: u64, names: &[(&str, bool)]) -> Meta {
    Meta {
        entries,
        unpacked,
        failed: false,
        names: names.iter().map(|(n, d)| (n.to_string(), *d)).collect(),
    }
}

// ql-archive board: "1 entry · 156 B unpacked" under the name.
#[test]
fn archive_line_reads_the_index() {
    let i18n = crate::i18n::test_catalogs::english();
    assert_eq!(
        line(&i18n, &meta(1, 156, &[("main.rs", false)])),
        "1 entry \u{b7} 156 B unpacked",
        "the board's line"
    );
    assert_eq!(
        line(&i18n, &meta(214, 3_400_000_000, &[])),
        "214 entries \u{b7} 3.4 GB unpacked",
        "a large archive"
    );
    assert_eq!(
        line(&i18n, &meta(1204, 0, &[])),
        "1,204 entries",
        "no unpacked total states the count alone"
    );
    assert_eq!(
        line(&i18n, &meta(0, 0, &[])),
        "0 entries",
        "an index that read cleanly and holds nothing is still a fact"
    );
    let failed = Meta {
        failed: true,
        ..meta(0, 0, &[])
    };
    assert_eq!(line(&i18n, &failed), "", "a failed index states nothing");
    assert_eq!(
        refusal(Some(&failed)),
        Some(crate::ui_logic_doc_preview::refusal::Refusal::ArchiveUnreadable),
        "and draws the archive sentence"
    );
    assert_eq!(
        refusal(Some(&meta(0, 0, &[]))),
        None,
        "an empty archive is not an error"
    );
    assert_eq!(
        refusal(None),
        None,
        "no answer yet is loading, not an error"
    );
}

// PreviewArchive.qml: as many rows as the frame holds less the "+ N more" line, which is always visible.
#[test]
fn archive_rows_are_bounded_by_the_wire_and_the_frame() {
    let i18n = crate::i18n::test_catalogs::english();
    assert_eq!(
        shown(22. * 5., 22.),
        4,
        "five line boxes list four rows and keep one for the count"
    );
    assert_eq!(shown(10., 22.), 0, "a frame under one line lists nothing");
    let m = meta(214, 0, &[("a", true), ("b", false), ("c", false)]);
    assert_eq!(entries(&m, 2).len(), 2, "the frame's room bounds the rows");
    assert_eq!(entries(&m, 10).len(), 3, "the wire's names bound the rows");
    assert_eq!(more(&m, 10), 211, "every entry not named is counted");
    assert_eq!(more_line(&i18n, &m, 2).as_deref(), Some("+ 212 more"));
    assert_eq!(
        more_line(&i18n, &meta(1, 156, &[("main.rs", false)]), 4),
        None,
        "a fully listed archive has no count line"
    );
    assert_eq!(
        more_line(&i18n, &meta(5000, 0, &[]), 0).as_deref(),
        Some("+ 5,000 more"),
        "the count is grouped"
    );
}
