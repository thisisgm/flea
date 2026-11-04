//! A replacement after validation must never receive the captured file's tags.
use super::*;
use crate::backend::{menu_actions::Selected, testdir::TestDir};

fn replace_after_validation(path: &Path) {
    std::fs::rename(path, path.with_file_name("original.txt")).unwrap();
    std::fs::write(path, "replacement").unwrap();
    super::super::tags::write_names(path, &["Blue".into()]).unwrap();
}

#[test]
fn ey379_tag_write_keeps_the_validated_inode_when_its_path_is_replaced() {
    let _guard = super::super::tags::SEAM
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let d = TestDir::new("menu-tags-swap");
    let path = d.file("selected.txt", "original");
    let selected = Selected::inspect(&path.to_string_lossy()).unwrap();
    let mut ops = tests::ops();
    let mut out = Vec::new();
    MENU_SWAP_HOOK.with(|slot| slot.set(Some(replace_after_validation)));
    apply_targets(
        &mut out,
        &mut ops,
        d.path(),
        vec![path.to_string_lossy().into_owned()],
        "add",
        "Red",
        Some(vec![selected]),
    );
    MENU_SWAP_HOOK.with(|slot| slot.set(None));
    assert_eq!(
        super::super::tags::read(&path).unwrap(),
        ["Blue"],
        "replacement stays untouched"
    );
    assert_eq!(
        super::super::tags::read(&d.join("original.txt")).unwrap(),
        ["Red"],
        "only the captured inode receives the tag"
    );
}

#[test]
fn ey379_bulk_listing_tags_keep_working_below_the_selection_descriptor_count() {
    const CHILD: &str = "FLEA_TAG_BULK_FD_CHILD";
    const FD_CAP: usize = 64;
    const FILES: usize = 1100;
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new("/bin/bash")
            .args(["-c", "ulimit -n \"$3\" || exit $?; exec \"$1\" --exact \"$2\" --test-threads=1", "tag-fd-proof"])
            .arg(std::env::current_exe().unwrap())
            .arg("backend::tagsreq::menu_tests::ey379_bulk_listing_tags_keep_working_below_the_selection_descriptor_count")
            .arg(FD_CAP.to_string())
            .env(CHILD, "1")
            .status().unwrap();
        assert!(status.success());
        return;
    }
    let d = TestDir::new("tag-bulk-descriptors");
    let mut state = tests::state_for(d.path());
    for index in 0..FILES {
        let name = format!("file-{index:04}.txt");
        d.file(&name, "body");
        state.listing.push(&name, false);
    }
    let mut ops = tests::ops();
    let mut out = Vec::new();
    apply_tags(
        &mut out,
        &mut ops,
        &state,
        (0..FILES).collect(),
        "add",
        "Red",
    );
    assert_eq!(
        String::from_utf8(out).unwrap(),
        settagged_line(FILES, "add") + "\n"
    );
    assert_eq!(ops.journal.len(), 1);
    assert_eq!(
        super::super::tags::read(&d.join("file-1099.txt")).unwrap(),
        ["Red"]
    );
}
