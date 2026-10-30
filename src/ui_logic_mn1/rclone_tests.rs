use super::{build, Context};

#[test]
fn vf6_rclone_row_is_configured_hideable_and_before_dropbox() {
    let mut context = Context {
        has_row: true,
        selection_count: 3,
        row_is_file: true,
        dropbox_installed: true,
        ..Context::default()
    };
    assert!(!build::listing_entries(&context)
        .iter()
        .any(|row| row.id.as_ref() == "sendRclone"));
    context.rclone_remotes = vec!["gdrive".into(), "Photos With Spaces".into()];
    let entries = build::listing_entries(&context);
    let at = entries
        .iter()
        .position(|row| row.id.as_ref() == "sendRclone")
        .unwrap();
    let dropbox = entries
        .iter()
        .position(|row| row.id.as_ref() == "dropbox")
        .unwrap();
    assert!(at < dropbox);
    assert_eq!(entries[at].glyph, Some("cloud-upload"));
    assert!(
        super::entry::MARK_NAMES.contains(&"cloud-upload"),
        "the row's mark is drawable, not the plain file fallback"
    );
    let remotes = entries[at].submenu.as_ref().unwrap();
    assert_eq!(
        remotes
            .iter()
            .map(|row| row.id.as_ref())
            .collect::<Vec<_>>(),
        ["gdrive", "Photos With Spaces"]
    );
    assert!(remotes.iter().all(|row| row.glyph == Some("server")));
    context.hidden.push("sendRclone".into());
    assert!(!build::listing_entries(&context)
        .iter()
        .any(|row| row.id.as_ref() == "sendRclone"));
}
