//! Quick Look asks an enabled previewer plugin for a file it cannot show and draws only its plain answer.
use super::preview_tests::{body, draw, file, opened, press};
use super::test_shell::{land, set_cursor};
use crate::{backend::Backend, backend::Event as BackendEvent, g2::pv1::Body};
use flea::{
    jsondoc::{self, Json},
    ui_logic_st2::{
        manifest,
        wire::{self, Listing, Plugin, Source},
    },
};
use gpui::TestAppContext;

fn previewer() -> Plugin {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugins/torrent-preview/manifest.json");
    Plugin {
        manifest: manifest::parse(&std::fs::read_to_string(path).unwrap(), false).unwrap(),
        source: Source::Folder("/plugins/torrent-preview".into()),
        enabled: true,
        failure: None,
        mark_path: String::new(),
        settings: Vec::new(),
        shelf: None,
    }
}

#[gpui::test]
fn quick_look_shows_a_previewer_plugin_text_for_an_unsupported_file(cx: &mut TestAppContext) {
    let (shell, cx, _pane) = opened(cx);
    let (owner, mut lines) = Backend::test_lines();
    let output = owner.test_output();
    cx.update(|_, cx| {
        crate::g2::plugins::service(cx).update(cx, |host, cx| host.test_attach(owner, cx))
    });
    output
        .try_send(BackendEvent::Plugins(wire::listing_line(&Listing {
            id: 0,
            plugins: vec![previewer()],
        })))
        .unwrap();
    draw(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("ubuntu.torrent", "application-x-bittorrent")],
        1,
    );
    set_cursor(&shell, cx, 0);
    press(cx, "space");
    assert!(
        matches!(body(&shell, cx), Some(Body::CannotShow { .. })),
        "the refusal shows first"
    );
    let asked: Vec<Json> = std::iter::from_fn(&mut lines)
        .filter_map(|line| jsondoc::parse(&line).ok())
        .filter(|d| {
            d.get("request")
                .and_then(|r| r.get("op"))
                .and_then(Json::as_str)
                == Some("preview")
        })
        .collect();
    assert_eq!(asked.len(), 1, "one preview ask for the cursor file");
    assert_eq!(
        asked[0]
            .get("request")
            .and_then(|r| r.get("paths"))
            .and_then(Json::as_array)
            .unwrap(),
        &[Json::Str("/files/ubuntu.torrent".into())]
    );
    let id = asked[0].get("id").and_then(Json::as_f64).unwrap();
    output
        .try_send(BackendEvent::Plugins(format!(
            r#"{{"t":"pluginruntime","id":{id},"plugin":"example.torrent-preview","ok":true,"message":{{"op":"preview","text":"ubuntu.iso\n<i>4.7 GB</i>"}},"progress":[],"code":"","error":""}}"#
        )))
        .unwrap();
    draw(cx);
    let Some(Body::Doc(doc)) = body(&shell, cx) else {
        panic!("the plugin text replaces the refusal")
    };
    let crate::g2::pv1::doc::View::Code(code) = &doc.view else {
        panic!("plain text view")
    };
    let shown = code.read_with(cx, |code, _| code.lines.plain_lines());
    assert_eq!(
        shown,
        vec!["ubuntu.iso".to_string(), "<i>4.7 GB</i>".to_string()],
        "markup stays literal"
    );
}
