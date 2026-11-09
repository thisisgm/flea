use super::*;
use crate::backend::{
    collide::Ask,
    opsdispatch::{self, Ops},
    testdir::TestDir,
};
use std::{
    path::Path,
    sync::{atomic::AtomicBool, mpsc::channel},
    time::Duration,
};

#[test]
fn held_paths_reply_is_one_stamped_protocol_line() {
    let root = TestDir::new("tui17-held-wire");
    let source = root.file("source", "held");
    let (tx, rx) = channel();
    paths(tx, vec![source.to_string_lossy().into_owned()], 9, 7);
    let OpMsg::Meta { line } = rx.recv_timeout(Duration::from_secs(10)).unwrap() else {
        panic!("held paths did not return metadata");
    };
    assert_eq!(line.lines().count(), 1);
    let reply = crate::jsondoc::parse(&line).unwrap();
    assert_eq!(reply.get("id").unwrap().as_f64(), Some(7.0));
    assert_eq!(reply.get("listing").unwrap().as_f64(), Some(9.0));
    assert_eq!(decode(reply.get("held").unwrap()).unwrap()[0].path, source);
}

#[test]
fn failed_held_paths_reply_keeps_its_request_and_listing() {
    let root = TestDir::new("tui17-held-error");
    let (tx, rx) = channel();
    paths(
        tx,
        vec![root.join("missing").to_string_lossy().into_owned()],
        9,
        7,
    );
    let OpMsg::Meta { line } = rx.recv_timeout(Duration::from_secs(10)).unwrap() else {
        panic!("held path failure did not return metadata");
    };
    assert_eq!(line.lines().count(), 1);
    let reply = crate::jsondoc::parse(&line).unwrap();
    assert_eq!(reply.get("t").unwrap().as_str(), Some("error"));
    assert_eq!(reply.get("id").unwrap().as_f64(), Some(7.0));
    assert_eq!(reply.get("listing").unwrap().as_f64(), Some(9.0));
}

#[test]
fn tui17_link_journal_child() {
    let Some(root) = std::env::var_os("FLEA_TUI17_LINK_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    for key in ["XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME"] {
        assert!(PathBuf::from(std::env::var_os(key).unwrap()).starts_with(&root));
    }
    let guard = |path: &Path| {
        assert!(path.is_absolute() && !path.as_os_str().is_empty() && path.starts_with(&root));
    };
    let source = root.join("source");
    guard(&source);
    std::fs::write(&source, "held").unwrap();
    let paths = vec![source.to_string_lossy().into_owned()];
    let held = decode(&encode(&capture(&paths).unwrap())).unwrap();
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx.clone());
    for op in ["absolute", "relative", "hard"] {
        let dest = root.join(op);
        guard(&dest);
        std::fs::create_dir(&dest).unwrap();
        let made = dest.join("source");
        guard(&made);
        opsdispatch::start_link_held(
            &mut Vec::new(),
            &mut ops,
            op,
            paths.clone(),
            &dest.to_string_lossy(),
            Ask::parse(r#"{"c":"link"}"#),
            Some(held.clone()),
        );
        let message = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let mut out = Vec::new();
        opsdispatch::report_op(&mut out, &mut ops, message);
        assert!(String::from_utf8_lossy(&out).contains("\"ok\":1"));
        if op == "hard" {
            assert_eq!(
                capture(&[made.to_string_lossy().into_owned()]).unwrap()[0]
                    .item
                    .parts(),
                held[0].item.parts()
            );
        } else {
            assert_eq!(
                std::fs::read_link(&made).unwrap(),
                if op == "absolute" {
                    source.clone()
                } else {
                    PathBuf::from("../source")
                }
            );
        }
        assert_eq!(ops.journal.undo().unwrap(), "link");
        assert!(made.symlink_metadata().is_err());
        assert_eq!(
            ops.journal.redo(1, &AtomicBool::new(false), &tx).unwrap(),
            "link"
        );
        assert!(made.symlink_metadata().is_ok());
    }
    let old = root.join("old-source");
    guard(&old);
    std::fs::rename(&source, old).unwrap();
    std::fs::write(&source, "replacement").unwrap();
    let dest = root.join("replaced");
    guard(&dest);
    std::fs::create_dir(&dest).unwrap();
    opsdispatch::start_link_held(
        &mut Vec::new(),
        &mut ops,
        "hard",
        paths,
        &dest.to_string_lossy(),
        Ask::parse(r#"{"c":"link"}"#),
        Some(held),
    );
    let message = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let mut out = Vec::new();
    opsdispatch::report_op(&mut out, &mut ops, message);
    assert!(String::from_utf8_lossy(&out).contains("changed"));
    assert!(dest.read_dir().unwrap().next().is_none());
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "replacement");
}
#[test]
fn a_raw_named_source_keeps_its_own_bytes_through_the_journal() {
    use flea::bytename::ExactText as _;
    use std::os::unix::ffi::OsStrExt;
    let root = TestDir::new("linkheld-raw");
    let raw = root.path().join(std::ffi::OsStr::from_bytes(b"src-\xff"));
    std::fs::write(&raw, "raw").unwrap();
    std::fs::write(root.path().join("src-\u{fffd}"), "twin").unwrap();
    let held = capture(&[raw.exact_text().into_owned()]).unwrap();
    let back = decode(&encode(&held)).unwrap();
    assert_eq!(back[0].path, raw);
    let inode = std::os::unix::fs::MetadataExt::ino(&std::fs::symlink_metadata(&raw).unwrap());
    assert_eq!(
        back[0].item.parts().1,
        inode,
        "the identity is the raw file's, never its twin's"
    );
}

#[test]
fn identities_survive_the_protocol_and_links_undo_redo_inside_the_fixture() {
    let root = TestDir::new("tui17-link-journal");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "backend::linkheld::tests::tui17_link_journal_child",
            "--nocapture",
        ])
        .env("FLEA_TUI17_LINK_ROOT", root.path())
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .status()
        .unwrap();
    assert!(status.success());
}
#[test]
fn a_changed_link_target_is_refused_and_a_relative_target_is_revealed_exactly() {
    let root = TestDir::new("tui17-follow");
    let target = root.file("target", "keep");
    let link = root.join("link");
    std::os::unix::fs::symlink("target", &link).unwrap();
    let mut out = Vec::new();
    opsdispatch::do_link_target_held(&mut out, &link.to_string_lossy(), 7, Some("target"));
    let value = crate::jsondoc::parse(String::from_utf8_lossy(&out).trim()).unwrap();
    assert_eq!(
        value.get("directory").unwrap().as_str(),
        root.path().to_str()
    );
    assert_eq!(
        value.get("name").unwrap().as_str(),
        target.file_name().unwrap().to_str()
    );
    out.clear();
    opsdispatch::do_link_target_held(&mut out, &link.to_string_lossy(), 7, Some("old"));
    assert!(String::from_utf8_lossy(&out).contains("changed"));
    assert_eq!(std::fs::read_to_string(target).unwrap(), "keep");
}
