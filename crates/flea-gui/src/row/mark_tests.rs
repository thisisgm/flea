use super::*;

#[test]
fn marks_follow_icons_js() {
    assert_eq!(Mark::for_row("folder", 0o120777), Mark::Symlink);
    assert_eq!(Mark::for_row("inode-directory", 0o040755), Mark::Folder);
    assert_eq!(Mark::for_row("text-html", 0o100644), Mark::Code);
    assert_eq!(Mark::for_row("x-office-document", 0o100644), Mark::FileText);
    assert_eq!(Mark::for_row("application-pdf", 0o100644), Mark::File);
    let (key, data) = Mark::Folder.svg();
    assert_eq!(key.as_ref(), "flea-mark:folder");
    let text = std::str::from_utf8(data).unwrap();
    assert!(
        text.starts_with("<svg ") && text.ends_with(r#"<path d="M2 20V3h6l2 3h12v14H2z"/></svg>"#)
    );
}

#[test]
fn clip_marks_answer_one_lookup_per_row_and_an_empty_clipboard_none() {
    let mut marks = ClipMarks::default();
    marks.scope("/w");
    assert_eq!(marks.mark_for("a"), None);
    assert_eq!(marks.lookups.get(), 0);
    marks.set(
        &["/w/a".into(), "/w/sub/b".into(), "/other/c".into()],
        false,
    );
    assert_eq!(marks.mark_for("a"), Some(ClipMark::Copy));
    assert_eq!(marks.mark_for("sub/b"), Some(ClipMark::Copy));
    assert_eq!(marks.mark_for("c"), None);
    assert_eq!(marks.lookups.get(), 3);
    marks.scope("/other/");
    assert_eq!(marks.mark_for("c"), Some(ClipMark::Copy));
    assert_eq!(marks.mark_for("a"), None);
    marks.set(&["/other/c".into()], true);
    assert_eq!(marks.mark_for("c"), Some(ClipMark::Cut));
    marks.set(&[], false);
    assert!(marks.is_empty() && marks.held() == 0);
    assert_eq!(marks.mark_for("c"), None);
    assert_eq!(marks.lookups.get(), 6);
    let (key, _) = ClipMark::Cut.svg();
    assert_eq!(key.as_ref(), "flea-mark:scissors");
}
