//! The column asks an enabled previewer plugin for a file no built-in preview draws, and draws only its plain answer.
use super::*;
use crate::{backend::Backend, backend::Event as BackendEvent, list::Typeface};
use flea::{
    jsondoc,
    ui_logic_plugin_settings::{
        manifest,
        wire::{self, Listing, Plugin, Source},
    },
};
use gpui::{TestAppContext, VisualTestContext};

const TORRENT: &str = "examples/plugins/torrent-preview/manifest.json";

fn previewer(enabled: bool) -> Plugin {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(TORRENT);
    Plugin {
        manifest: manifest::parse(&std::fs::read_to_string(path).unwrap(), false).unwrap(),
        source: Source::Folder("/plugins/torrent-preview".into()),
        enabled,
        failure: None,
        mark_path: String::new(),
        settings: Vec::new(),
        shelf: None,
    }
}

fn subject(name: &str, icon: &str) -> Subject {
    Subject {
        directory: "/fixture".into(),
        listing: 1,
        index: 0,
        row: RowFacts {
            name: name.into(),
            icon: icon.into(),
            mode: 0o100644,
            size: 120,
            ..Default::default()
        },
        selected: Vec::new(),
        selected_count: 1,
    }
}

struct Rig {
    preview: Entity<SelectionPreview>,
    output: async_channel::Sender<BackendEvent>,
    lines: Box<dyn FnMut() -> Option<String>>,
}

fn rig(cx: &mut TestAppContext, plugin: Plugin) -> (Rig, &mut VisualTestContext) {
    let (backend, lines) = Backend::test_lines();
    let output = backend.test_output();
    let (preview, cx) = cx.add_window_view(|_, cx| {
        let host = crate::g2::plugins::service(cx);
        host.update(cx, |host, cx| host.test_attach(backend, cx));
        let mut view = SelectionPreview::new(
            Port::default(),
            Arc::new(crate::test_catalogs::english()),
            Typeface::omarchy_default(),
            cx,
        );
        view.width = 280.;
        view.height = 600.;
        view.visible = true;
        view
    });
    let listing = wire::listing_line(&Listing {
        id: 0,
        plugins: vec![plugin],
    });
    output.try_send(BackendEvent::Plugins(listing)).unwrap();
    cx.run_until_parked();
    (
        Rig {
            preview,
            output,
            lines: Box::new(lines),
        },
        cx,
    )
}

impl Rig {
    fn previews(&mut self) -> Vec<Json> {
        std::iter::from_fn(|| (self.lines)())
            .filter_map(|line| jsondoc::parse(&line).ok())
            .filter(|d| {
                d.get("request")
                    .and_then(|r| r.get("op"))
                    .and_then(Json::as_str)
                    == Some("preview")
            })
            .collect()
    }
    fn answer(&self, asked: &Json, message: &str, cx: &mut VisualTestContext) {
        let id = asked.get("id").and_then(Json::as_f64).unwrap();
        let line = format!(
            r#"{{"t":"pluginruntime","id":{id},"plugin":"example.torrent-preview","ok":true,"message":{message},"progress":[],"code":"","error":""}}"#
        );
        self.output.try_send(BackendEvent::Plugins(line)).unwrap();
        cx.run_until_parked();
    }
    fn follow(&self, subject: Subject, cx: &mut VisualTestContext) {
        cx.update(|window, cx| {
            self.preview.update(cx, |v, cx| {
                v.follow(Some(subject), false, false, window, cx)
            })
        });
        cx.run_until_parked();
    }
}

#[gpui::test]
fn plugin_preview_column_draws_the_plugin_text_as_plain_lines(cx: &mut TestAppContext) {
    let (mut rig, cx) = rig(cx, previewer(true));
    rig.follow(subject("ubuntu.torrent", "application-x-bittorrent"), cx);
    let asked = rig.previews();
    assert_eq!(asked.len(), 1, "one preview ask for the selected file");
    let request = asked[0].get("request").unwrap();
    assert_eq!(
        asked[0].get("plugin").and_then(Json::as_str),
        Some("example.torrent-preview")
    );
    assert_eq!(
        request.get("paths").and_then(Json::as_array).unwrap(),
        &[Json::Str("/fixture/ubuntu.torrent".into())]
    );
    assert!(
        request.get("folder").is_none(),
        "a preview receives only the selected file"
    );
    let held = rig.preview.read_with(cx, |v, _| {
        v.pending.as_ref().is_some_and(|f| !f.thumb_answered)
    });
    assert!(held, "the frame waits for the plugin's answer");
    rig.answer(
        &asked[0],
        r#"{"op":"preview","text":"Name: ubuntu.iso\n<b>Files</b>: 1"}"#,
        cx,
    );
    let lines = rig.preview.read_with(cx, |v, cx| {
        let frame = v.pending.as_ref().or(v.drawn.as_ref()).unwrap();
        assert!(frame.thumb_answered);
        match &frame.mounted {
            Mounted::Doc(doc) => doc.lines(cx),
            _ => panic!("the plugin text is not mounted in the text view"),
        }
    });
    assert_eq!(
        lines,
        vec![
            "Name: ubuntu.iso".to_string(),
            "<b>Files</b>: 1".to_string()
        ],
        "markup stays literal text"
    );
}

#[gpui::test]
fn plugin_preview_column_shows_the_returned_picture(cx: &mut TestAppContext) {
    let (mut rig, cx) = rig(cx, previewer(true));
    rig.follow(subject("ubuntu.torrent", "application-x-bittorrent"), cx);
    let asked = rig.previews();
    // A real picture, because drawing clears a source that fails to load.
    let png = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/images/empty.png");
    rig.answer(
        &asked[0],
        &format!(r#"{{"op":"preview","png":"{png}","width":4,"height":3}}"#),
        cx,
    );
    let (source, failed) = rig.preview.read_with(cx, |v, _| {
        let frame = v.pending.as_ref().or(v.drawn.as_ref()).unwrap();
        assert!(frame.thumb_answered);
        (frame.source.clone(), frame.failed)
    });
    assert!(
        source.is_some() && !failed,
        "the picture becomes the frame's source"
    );
}

#[gpui::test]
fn plugin_preview_column_never_asks_for_builtin_kinds(cx: &mut TestAppContext) {
    let (mut rig, cx) = rig(cx, previewer(true));
    rig.follow(subject("notes.txt", "text-plain"), cx);
    assert!(
        rig.previews().is_empty(),
        "a built-in text preview stays the column's own"
    );
}

#[gpui::test]
fn plugin_preview_column_never_asks_a_disabled_previewer(cx: &mut TestAppContext) {
    let (mut rig, cx) = rig(cx, previewer(false));
    rig.follow(subject("ubuntu.torrent", "application-x-bittorrent"), cx);
    assert!(
        rig.previews().is_empty(),
        "a disabled previewer is never asked"
    );
}

#[gpui::test]
fn plugin_preview_column_refusal_releases_the_frame_without_a_view(cx: &mut TestAppContext) {
    let (mut rig, cx) = rig(cx, previewer(true));
    rig.follow(subject("ubuntu.torrent", "application-x-bittorrent"), cx);
    let asked = rig.previews();
    let id = asked[0].get("id").and_then(Json::as_f64).unwrap();
    let line = format!(
        r#"{{"t":"pluginruntime","id":{id},"plugin":"example.torrent-preview","ok":false,"message":null,"progress":[],"code":"start","error":"plugin timed out"}}"#
    );
    rig.output.try_send(BackendEvent::Plugins(line)).unwrap();
    cx.run_until_parked();
    rig.preview.read_with(cx, |v, _| {
        let frame = v.pending.as_ref().or(v.drawn.as_ref()).unwrap();
        assert!(
            frame.thumb_answered,
            "a failed plugin never holds the frame"
        );
        assert!(matches!(frame.mounted, Mounted::Basic) && frame.source.is_none());
    });
}

#[gpui::test]
fn plugin_preview_column_asks_when_the_listing_lands_after_the_frame(cx: &mut TestAppContext) {
    let (backend, lines) = Backend::test_lines();
    let output = backend.test_output();
    let (preview, cx) = cx.add_window_view(|_, cx| {
        crate::g2::plugins::service(cx).update(cx, |host, cx| host.test_attach(backend, cx));
        let mut view = SelectionPreview::new(
            Port::default(),
            Arc::new(crate::test_catalogs::english()),
            Typeface::omarchy_default(),
            cx,
        );
        (view.width, view.height, view.visible) = (280., 600., true);
        view
    });
    let mut rig = Rig {
        preview,
        output,
        lines: Box::new(lines),
    };
    rig.follow(subject("ubuntu.torrent", "application-x-bittorrent"), cx);
    assert!(rig.previews().is_empty(), "no previewer is listed yet");
    let listing = wire::listing_line(&Listing {
        id: 0,
        plugins: vec![previewer(true)],
    });
    rig.output.try_send(BackendEvent::Plugins(listing)).unwrap();
    cx.run_until_parked();
    let asked = rig.previews();
    assert_eq!(
        asked.len(),
        1,
        "the restored selection asks once the listing lands"
    );
    rig.answer(
        &asked[0],
        r#"{"op":"preview","text":"Name: ubuntu.iso"}"#,
        cx,
    );
    let mounted = rig.preview.read_with(cx, |v, _| {
        let frame = v.pending.as_ref().or(v.drawn.as_ref()).unwrap();
        matches!(frame.mounted, Mounted::Doc(_))
    });
    assert!(mounted, "the late answer reaches the frame");
    rig.output
        .try_send(BackendEvent::Plugins(wire::listing_line(&Listing {
            id: 1,
            plugins: vec![previewer(true)],
        })))
        .unwrap();
    cx.run_until_parked();
    assert!(
        rig.previews().is_empty(),
        "one ask per frame, however often the host changes"
    );
}
