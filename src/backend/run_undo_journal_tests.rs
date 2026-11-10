// S1B-7: the single-item Permissions card's apply is journalled, so z restores its mode and never undoes an older write.
use super::*;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

fn mode(path: &Path) -> u32 {
    path.metadata().unwrap().mode() & 0o777
}

#[test]
fn a_single_permissions_apply_is_what_z_undoes_and_redo_replays() {
    let mut h = Loop::new("undo-single-permissions");
    let parent = h.dir.path().to_path_buf();
    h.request(&format!(
        r#"{{"c":"mkdir","path":"{}","name":"older"}}"#,
        parent.display()
    ));
    assert!(h.said().contains(r#""ok":true"#));
    let item = h.dir.file("mode.sh", "#!/bin/sh\n");
    std::fs::set_permissions(&item, std::fs::Permissions::from_mode(0o644)).unwrap();
    h.request(&format!(
        r#"{{"c":"permissions","op":"inspect","id":7,"path":"{}"}}"#,
        item.display()
    ));
    assert!(h.said().contains(r#""ok":true"#));
    h.request(r#"{"c":"permissions","op":"apply","id":7,"mode":"755"}"#);
    assert!(h.said().contains(r#""op":"apply","ok":true"#));
    assert_eq!(mode(&item), 0o755);
    h.request(r#"{"c":"undo"}"#);
    assert!(h
        .said()
        .contains(r#"{"t":"undone","op":"permissions","ok":true}"#));
    assert_eq!(mode(&item), 0o644, "z restores the mode the card changed");
    assert!(
        h.dir.join("older").is_dir(),
        "the older new folder stays until its own z"
    );
    h.request(r#"{"c":"redo"}"#);
    let started = h.said();
    assert!(
        started.contains(r#""t":"redostarted""#) && started.contains(r#""op":"permissions""#),
        "{started}"
    );
    let done = h
        ._ops
        .recv_timeout(EVENT_DEADLINE)
        .expect("the redo worker reports back");
    h.feed(Event::Op(done));
    assert!(h
        .said()
        .contains(r#""t":"redone","op":"permissions","ok":true"#));
    assert_eq!(mode(&item), 0o755, "redo replays the change");
}

#[test]
fn a_refused_permissions_apply_journals_nothing() {
    let mut h = Loop::new("undo-refused-permissions");
    let item = h.dir.file("item", "a");
    std::fs::set_permissions(&item, std::fs::Permissions::from_mode(0o644)).unwrap();
    h.request(&format!(
        r#"{{"c":"permissions","op":"inspect","id":3,"path":"{}"}}"#,
        item.display()
    ));
    assert!(h.said().contains(r#""op":"inspect","ok":true"#));
    h.request(r#"{"c":"permissions","op":"apply","id":3,"mode":"9"}"#);
    let refused = h.said();
    assert!(
        refused.contains(r#""ok":false"#) && refused.contains("Enter three octal digits"),
        "{refused}"
    );
    let moved = h.dir.join("moved");
    std::fs::rename(&item, &moved).unwrap();
    h.request(r#"{"c":"permissions","op":"apply","id":3,"mode":"755"}"#);
    let gone = h.said();
    assert!(
        gone.contains(r#""ok":false"#) && gone.contains("moved or disappeared"),
        "{gone}"
    );
    h.request(r#"{"c":"undo"}"#);
    assert!(
        h.said().contains("nothing to undo"),
        "a refused apply leaves nothing for z"
    );
    assert_eq!(mode(&moved), 0o644);
}
