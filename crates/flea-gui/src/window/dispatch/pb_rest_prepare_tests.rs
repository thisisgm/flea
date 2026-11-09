//! A rested local Markdown row prepares off-thread and hands the parsed view to Space.
use super::search_open_tests::line;
use super::test_shell::{land, open_root_at};
use crate::{
    g2::quick_look_overlay::{doc::View, Body},
    state::Shell,
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let parent = std::env::temp_dir().join("PB-REST-1");
        std::fs::create_dir_all(&parent).unwrap();
        let output = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("prepare.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(root.is_absolute() && root.starts_with(parent));
        std::fs::write(
            root.join(".flea-test-sandbox"),
            "PB-REST-1 Markdown preparation\n",
        )
        .unwrap();
        std::fs::write(
            root.join("readme.md"),
            "# Prepared\n\nA bounded local document.\n",
        )
        .unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let parent = std::env::temp_dir().join("PB-REST-1");
        assert!(
            self.0.is_absolute()
                && self.0.starts_with(parent)
                && self.0.join(".flea-test-sandbox").is_file()
        );
        if !std::thread::panicking() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn open<'a>(
    fixture: &Fixture,
    cx: &'a mut TestAppContext,
    class: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    cx.executor().allow_parking();
    let path = fixture.0.to_str().unwrap();
    let (shell, cx) = open_root_at(cx, path);
    let mut file = super::preview_tests::file("readme.md", "text-markdown");
    file.size = std::fs::metadata(fixture.0.join("readme.md"))
        .unwrap()
        .len();
    land(
        &shell,
        cx,
        path,
        vec![file, super::preview_tests::file("next.txt", "text-plain")],
        1,
    );
    line(
        &shell,
        cx,
        &format!(r#"{{"t":"fsinfo","path":"{path}","fs":"ext4","free":1000,"class":"{class}"}}"#),
    );
    cx.executor().advance_clock(Duration::from_millis(120));
    super::menus_tests::settle(cx);
    (shell, cx)
}

fn ready(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> gpui::EntityId {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let prepared = shell.read_with(cx, |shell, cx| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .look_warm
                .prepared
                .as_ref()
                .map(|doc| {
                    let View::Markdown(view) = &doc.view else {
                        panic!("a Markdown preparation")
                    };
                    let view = view.read(cx);
                    (doc.id(), view.doc.complete && view.counts.parses == 1)
                })
        });
        if let Some((id, true)) = prepared {
            return id;
        }
        assert!(
            Instant::now() < until,
            "the rested local document never prepared: {prepared:?}"
        );
        super::preview_tests::draw(cx);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[gpui::test]
fn pb_rest_markdown_rest_prepares_once_and_space_reuses_that_parse(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (shell, cx) = open(&fixture, cx, "local");
    let prepared = ready(&shell, cx);
    cx.simulate_keystrokes("space");
    super::menus_tests::settle(cx);
    shell.read_with(cx, |shell, cx| {
        let host = shell.quick_look.as_ref().unwrap();
        let Body::Doc(doc) = &host.view.read(cx).subject().unwrap().body else {
            panic!("Space mounted a document")
        };
        assert_eq!(doc.id(), prepared, "Space takes the prepared entity");
        let View::Markdown(view) = &doc.view else {
            panic!("Space mounted Markdown")
        };
        assert_eq!(
            view.read(cx).counts.parses,
            1,
            "the unchanged document is not reparsed"
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_network_markdown_never_prepares_and_a_cursor_move_drops_the_entry(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let (shell, cx) = open(&fixture, cx, "network");
    shell.read_with(cx, |shell, _| {
        assert!(shell.panes[0]
            .as_ref()
            .unwrap()
            .look_warm
            .prepared
            .is_none())
    });
    line(
        &shell,
        cx,
        &format!(
            r#"{{"t":"fsinfo","path":"{}","fs":"ext4","free":1000,"class":"local"}}"#,
            fixture.0.display()
        ),
    );
    cx.executor().advance_clock(Duration::from_millis(120));
    super::menus_tests::settle(cx);
    ready(&shell, cx);
    cx.simulate_keystrokes("down");
    super::preview_tests::draw(cx);
    shell.read_with(cx, |shell, _| {
        assert!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .look_warm
                .prepared
                .is_none(),
            "moving releases the prepared entry"
        )
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_space_revalidates_prepared_text_even_when_the_listed_size_is_unchanged(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let (shell, cx) = open(&fixture, cx, "local");
    ready(&shell, cx);
    let path = fixture.0.join("readme.md");
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("Prepared", "Changed!");
    std::fs::write(path, text).unwrap();
    cx.simulate_keystrokes("space");
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let fresh = shell.read_with(cx, |shell, cx| {
            let host = shell.quick_look.as_ref().unwrap();
            let Body::Doc(doc) = &host.view.read(cx).subject().unwrap().body else {
                return false;
            };
            let View::Markdown(view) = &doc.view else {
                return false;
            };
            let view = view.read(cx);
            view.doc.complete
                && view
                    .doc
                    .text
                    .as_ref()
                    .is_some_and(|text| text.contains("Changed!"))
                && view.counts.parses == 2
        });
        if fresh {
            break;
        }
        assert!(Instant::now() < until, "Space kept a stale prepared parse");
        super::preview_tests::draw(cx);
        std::thread::sleep(Duration::from_millis(5));
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn previewq_dismiss_does_not_prepare_the_unchanged_markdown_again(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (shell, cx) = open(&fixture, cx, "local");
    let prepared = ready(&shell, cx);
    cx.simulate_keystrokes("space");
    super::menus_tests::settle(cx);
    let shown = shell.read_with(cx, |shell, cx| {
        assert!(shell.quick_look_open(cx), "Space opened Quick Look");
        let host = shell.quick_look.as_ref().unwrap();
        let Body::Doc(doc) = &host.view.read(cx).subject().unwrap().body else {
            panic!("Space mounted a document")
        };
        let View::Markdown(view) = &doc.view else {
            panic!("Space mounted Markdown")
        };
        (doc.id(), view.read(cx).doc.seq)
    });
    assert_eq!(shown.0, prepared, "Space took the rested preparation");
    cx.simulate_keystrokes("escape");
    super::menus_tests::settle(cx);
    cx.executor().advance_clock(Duration::from_millis(150));
    super::menus_tests::settle(cx);
    shell.read_with(cx, |shell, cx| {
        assert!(!shell.quick_look_open(cx), "Escape closed Quick Look");
        let doc = shell.panes[0].as_ref().unwrap().look_warm.prepared.as_ref();
        let kept = doc.map(|doc| {
            let View::Markdown(view) = &doc.view else {
                panic!("a Markdown entry")
            };
            let view = view.read(cx);
            (
                doc.id(),
                view.doc.seq,
                view.counts.parses,
                view.doc.complete,
            )
        });
        assert_eq!(
            kept,
            Some((shown.0, shown.1, 1, true)),
            "dismissing an unchanged document must not read and prepare it again"
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn previewq_dismiss_then_space_reopens_the_unchanged_markdown_warm(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (shell, cx) = open(&fixture, cx, "local");
    let shown = ready(&shell, cx);
    cx.simulate_keystrokes("space");
    super::menus_tests::settle(cx);
    assert!(shell.read_with(cx, |shell, cx| shell.quick_look_open(cx)));
    cx.simulate_keystrokes("escape");
    // No clock advance: a re-preparation could only land after its 120 ms timer.
    super::preview_tests::draw(cx);
    let warm = shell.read_with(cx, |shell, cx| {
        assert!(!shell.quick_look_open(cx), "Escape closed Quick Look");
        let warm = &shell.panes[0].as_ref().unwrap().look_warm;
        warm.prepared.as_ref().map(|doc| doc.id())
    });
    assert_eq!(
        warm,
        Some(shown),
        "the dismiss hands the shown view back at once"
    );
    cx.simulate_keystrokes("space");
    super::menus_tests::settle(cx);
    shell.read_with(cx, |shell, cx| {
        let host = shell.quick_look.as_ref().unwrap();
        let Body::Doc(doc) = &host.view.read(cx).subject().unwrap().body else {
            panic!("Space mounted a document")
        };
        assert_eq!(doc.id(), shown, "the second Space takes the shown view");
        let View::Markdown(view) = &doc.view else {
            panic!("Space mounted Markdown")
        };
        assert_eq!(view.read(cx).counts.parses, 1, "the reopen is not reparsed");
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[path = "pb_rest_prepare_assets_tests.rs"]
mod assets;
