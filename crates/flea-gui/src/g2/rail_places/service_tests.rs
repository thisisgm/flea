// The places service against a marked scratch root: the history read and the Favorites writer, split from tests.rs.
use super::service::{read_history, Places};
use flea::i18n::{I18n, Paths};
use flea::ui_logic_rail_places::favourites::Operation;
use gpui::{AppContext, TestAppContext};
use std::path::PathBuf;
use std::sync::Arc;

fn english() -> I18n {
    I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent"),
        language_file: PathBuf::from("/nonexistent"),
        catalogs: vec![crate::test_catalogs::english_only()],
    })
}

// A marked root of this test's own, created exclusively under a fresh name and removed on a pass only with its marker in temp_dir.
struct Scratch(PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;

    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            remove_marked(&self.0);
        }
    }
}

// Deletes a root only when it is absolute, named, holds its marker and sits directly in temp_dir.
fn remove_marked(dir: &std::path::Path) {
    let parent = std::env::temp_dir().canonicalize().ok();
    let ours = dir.is_absolute()
        && dir.file_name().is_some()
        && dir.join(".flea-test-sandbox").is_file()
        && dir.parent().and_then(|path| path.canonicalize().ok()) == parent;
    if ours {
        std::fs::remove_dir_all(dir).unwrap();
    }
}

fn scratch(name: &str) -> Scratch {
    let base = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    for attempt in 0..64u32 {
        let dir = base.join(format!(
            "flea-g2-pl4-{name}-{}-{nanos}-{attempt}",
            std::process::id()
        ));
        if std::fs::create_dir(&dir).is_ok() {
            std::fs::write(dir.join(".flea-test-sandbox"), "g2-pl4\n").unwrap();
            return Scratch(dir);
        }
    }
    panic!("no fresh scratch name under {}", base.display());
}

#[test]
fn history_reads_newest_first_and_a_missing_file_is_empty() {
    let dir = scratch("history");
    let file = dir.join("recently-used.xbel");
    std::fs::write(&file, r#"<xbel><bookmark href="file:///a" visited="2026-08-01T00:00:00Z"/><bookmark href="file:///b" visited="2026-08-30T00:00:00Z"/></xbel>"#).unwrap();
    let read = read_history(file.to_str().unwrap());
    assert_eq!(
        read.iter().map(|v| v.path.as_str()).collect::<Vec<_>>(),
        ["/b", "/a"]
    );
    assert!(read_history(dir.join("absent").to_str().unwrap()).is_empty());
    drop(dir);
}

#[gpui::test]
fn writes_run_off_the_ui_thread_and_say_their_failure(cx: &mut TestAppContext) {
    let catalog = Arc::new(english());
    let places = cx.new(|_| Places::new(catalog.clone(), PathBuf::from("/bin/false")));
    places.update(cx, |p, cx| {
        p.write(
            &Operation::Add {
                label: "Docs".into(),
                path: "/d".into(),
            },
            cx,
        )
    });
    places.update(cx, |p, cx| p.write(&Operation::Remove { index: 0 }, cx));
    assert_eq!(
        places.read_with(cx, |p, _| p.said.clone()).as_deref(),
        Some("A Favorites change is still being saved."),
        "one write at a time"
    );
    cx.run_until_parked();
    let (said, busy) = places.read_with(cx, |p, _| (p.said.clone(), p.favourites.busy));
    assert_eq!(
        (said.as_deref(), busy),
        (Some("Favorites could not be saved."), false),
        "a bad exit with nothing said"
    );
    let dir = scratch("reread");
    let file = dir.join("recently-used.xbel");
    std::fs::write(
        &file,
        r#"<xbel><bookmark href="file:///a" visited="2026-08-01T00:00:00Z"/></xbel>"#,
    )
    .unwrap();
    places.update(cx, |p, cx| {
        p.reread_history(file.to_string_lossy().into_owned(), cx)
    });
    cx.run_until_parked();
    assert_eq!(
        places.read_with(cx, |p, _| (p.history.len(), p.history_reads)),
        (1, 1)
    );
    drop(dir);
}

#[test]
fn a_scratch_root_never_deletes_what_stood_at_a_predictable_name() {
    // The squat takes the name the old scratch deleted first; it is created exclusively and marked, so only remove_marked deletes it.
    let squat =
        Scratch(std::env::temp_dir().join(format!("flea-g2-pl4-canary-{}", std::process::id())));
    std::fs::create_dir(&squat.0).unwrap();
    std::fs::write(squat.join(".flea-test-sandbox"), "g2-pl4\n").unwrap();
    std::fs::write(squat.join("keep"), b"not the scratch's").unwrap();
    let dir = scratch("canary");
    assert!(
        squat.join("keep").is_file(),
        "the scratch root deleted a directory it did not create"
    );
    assert_ne!(*dir, *squat, "the scratch root is a fresh name");
}
