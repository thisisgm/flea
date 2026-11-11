//! GAP170: apply relative owner execute to current modes and journal those exact modes.
use super::fixture::{Fixture, Process};
use flea::jsondoc::{self, Json};
use gpui::TestAppContext;
use std::{os::unix::fs::PermissionsExt, path::Path};

fn chmod(fixture: &Fixture, name: &str, bits: u32) {
    assert!(fixture.root.is_absolute() && fixture.root.join(".flea-test-sandbox").is_file());
    std::fs::set_permissions(
        fixture.root.join(name),
        std::fs::Permissions::from_mode(bits),
    )
    .unwrap();
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

fn ok(reply: &str) {
    assert_eq!(
        jsondoc::parse(reply)
            .unwrap()
            .get("ok")
            .and_then(Json::as_bool),
        Some(true),
        "{reply}"
    );
}

#[gpui::test]
fn sol_makeexec_relative_owner_execute_preserves_chmod_before_choice_and_before_apply(
    cx: &mut TestAppContext,
) {
    for after_choice in [false, true] {
        let mut fixture = Fixture::new(cx, 0o644);
        let request = fixture.open(cx, 0, vec![]).unwrap();
        fixture.probe(cx, &request);
        if !after_choice {
            chmod(&fixture, "build.sh", 0o600);
        }
        let batch = fixture.choose(cx);
        if after_choice {
            chmod(&fixture, "build.sh", 0o600);
        }
        let mut backend = Process::new(&fixture.root);
        let reply = backend.request(&batch, "permissions");
        ok(&reply);
        assert_eq!(mode(&fixture.root.join("build.sh")), 0o700);
        fixture.receive(cx, &reply);
        let undo = jsondoc::parse(r#"{"c":"undo"}"#).unwrap();
        backend.request(&undo, "undone");
        assert_eq!(mode(&fixture.root.join("build.sh")), 0o600);
    }
}

#[gpui::test]
fn sol_makeexec_batch_resolves_each_files_current_mode_and_undo_restores_each(
    cx: &mut TestAppContext,
) {
    let mut fixture = Fixture::new(cx, 0o644);
    let request = fixture.open(cx, 0, vec![]).unwrap();
    fixture.probe(cx, &request);
    let chosen = fixture.choose(cx);
    let operation = chosen.get("modes").unwrap().as_array().unwrap()[0]
        .as_str()
        .unwrap();
    let paths: Vec<String> = ["build.sh", "notes.txt"]
        .iter()
        .map(|name| fixture.root.join(name).to_string_lossy().into_owned())
        .collect();
    let batch = jsondoc::parse(&format!(
        r#"{{"c":"permissionsBatch","id":1,"paths":{},"modes":{}}}"#,
        flea::json::string_array(&paths),
        flea::json::string_array(&[operation.to_owned(), operation.to_owned()])
    ))
    .unwrap();
    chmod(&fixture, "build.sh", 0o600);
    chmod(&fixture, "notes.txt", 0o400);
    let mut backend = Process::new(&fixture.root);
    ok(&backend.request(&batch, "permissions"));
    assert_eq!(mode(&fixture.root.join("build.sh")), 0o700);
    assert_eq!(mode(&fixture.root.join("notes.txt")), 0o500);
    let undo = jsondoc::parse(r#"{"c":"undo"}"#).unwrap();
    backend.request(&undo, "undone");
    assert_eq!(mode(&fixture.root.join("build.sh")), 0o600);
    assert_eq!(mode(&fixture.root.join("notes.txt")), 0o400);
    let redo = jsondoc::parse(r#"{"c":"redo"}"#).unwrap();
    backend.request(&redo, "redone");
    assert_eq!(mode(&fixture.root.join("build.sh")), 0o700);
    assert_eq!(mode(&fixture.root.join("notes.txt")), 0o500);
}

#[gpui::test]
fn sol_makeexec_link_swapped_after_capture_leaves_its_target_untouched(cx: &mut TestAppContext) {
    let mut fixture = Fixture::new(cx, 0o644);
    let request = fixture.open(cx, 0, vec![]).unwrap();
    fixture.probe(cx, &request);
    let batch = fixture.choose(cx);
    assert!(fixture.root.is_absolute() && fixture.root.join(".flea-test-sandbox").is_file());
    std::fs::rename(
        fixture.root.join("build.sh"),
        fixture.root.join("original.sh"),
    )
    .unwrap();
    std::os::unix::fs::symlink("notes.txt", fixture.root.join("build.sh")).unwrap();
    chmod(&fixture, "notes.txt", 0o600);
    let reply = Process::new(&fixture.root).request(&batch, "permissions");
    assert_eq!(
        jsondoc::parse(&reply)
            .unwrap()
            .get("error")
            .and_then(Json::as_str),
        Some("Permissions takes one item, not a link.")
    );
    assert_eq!(mode(&fixture.root.join("notes.txt")), 0o600);
    assert_eq!(mode(&fixture.root.join("original.sh")), 0o644);
}
