use super::*;

/// Verify that enabling the accelerator invalidates old failure markers.
#[test]
fn the_fingerprint_changes_when_the_optional_jpeg_route_changes() {
    let mut specs = HashMap::new();
    specs.insert(
        JPEG_MIME.to_string(),
        Spec {
            exec: GLYCIN_JPEG_EXEC.map(String::from).to_vec(),
        },
    );
    assert_ne!(fingerprint_of(&specs, false), fingerprint_of(&specs, true));
}

/// The accelerator is limited to the stock provider when it wins the JPEG MIME entry.
#[test]
fn optional_jpeg_routing_preserves_the_winning_provider() {
    let aliases = Aliases::from_str("");
    let entry = |name: &str, exec: &str| {
        (
            name.to_owned(),
            format!("[Thumbnailer Entry]\nExec={exec}\nMimeType=image/jpeg;\n"),
        )
    };
    let stock = entry(SYSTEM_GLYCIN_SPEC, &GLYCIN_JPEG_EXEC.join(" "));
    let changed = entry(
        SYSTEM_GLYCIN_SPEC,
        "/usr/bin/glycin-thumbnailer --input %u --output %o --size=128",
    );
    let user = entry(
        "/home/user/.local/share/thumbnailers/photos.thumbnailer",
        "/bin/custom %i %o",
    );
    let table = |entries: &[_], vips| {
        Thumbnailers::from_entries_with_vips(entries, &aliases, vips, |path| Some(path.to_owned()))
    };
    assert!(table(std::slice::from_ref(&stock), true).can_speed_up_jpeg(JPEG_MIME, &aliases));
    assert!(!table(std::slice::from_ref(&stock), false).can_speed_up_jpeg(JPEG_MIME, &aliases));
    assert!(!table(&[changed], true).can_speed_up_jpeg(JPEG_MIME, &aliases));
    let customized = table(&[user, stock], true);
    assert!(!customized.can_speed_up_jpeg(JPEG_MIME, &aliases));
    assert_eq!(
        customized.for_mime(JPEG_MIME, &aliases).unwrap().exec[0],
        "/bin/custom"
    );
}
