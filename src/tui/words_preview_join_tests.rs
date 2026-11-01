use super::tests::Fixture;
use crate::{
    jsondoc::{self, Json},
    tui::{
        actions,
        graphics::Protocol,
        input::{Key, Pointer},
        keymap::Map,
        layout,
        media::Player,
        model::{Model, Row},
        picture, preview, wire,
    },
};

fn fixture() -> Fixture {
    let f = Fixture::new();
    f.write("catalog/en/flea.json", r#"{"preview.loading":"Loading","preview.loadHint":"%1 to load preview","preview.unreadable":"Unreadable","pane.unread":"Cannot read folder","media.play":"Play","media.pause":"Pause","properties.size":"Size","paste.nothing":"Nothing","tui.model.empty_folder":"Empty folder"}"#);
    f.write("catalog/ja/flea.json", r#"{"preview.loading":"読み込み中","preview.loadHint":"%1 で読み込み","preview.unreadable":"読めません","pane.unread":"フォルダを読めません","media.play":"とても長い再生ボタン","media.pause":"一時停止ボタン","properties.size":"容量","paste.nothing":"空です","tui.model.empty_folder":"空のフォルダ"}"#);
    f.write(
        "catalog/ar/flea.json",
        r#"{"media.play":"تشغيل طويل جدا","media.pause":"إيقاف مؤقت طويل"}"#,
    );
    f
}
fn model(f: &Fixture) -> Model {
    let mut m = f.model();
    m.rows.insert(
        0,
        Row::parse(
            &jsondoc::parse(r#"{"n":"clip.mp4","d":false,"s":4,"i":"video-mp4"}"#).unwrap(),
            &[],
        ),
    );
    m.preview_visible = true;
    m.preview_path = m.current_path().unwrap();
    m.preview = vec!["clip.mp4".into(), "literal kind".into()];
    preview::refresh_locale(&mut m);
    m
}

#[test]
fn tui_words_preview_join_frame_projects_both_consumers_from_one_snapshot() {
    let f = fixture();
    let mut m = model(&f);
    m.action_id = 11;
    m.sheet = true;
    m.properties = Some(Vec::new());
    let reply = jsondoc::parse(
        r#"{"t":"menuaction","op":"properties","id":11,"ok":true,"path":"/fx/raw.txt","bytes":4}"#,
    )
    .unwrap();
    let (mut backend, reader) = wire::echo_wire();
    m.receive(reply, &mut backend).unwrap();
    wire::finish(backend, reader);
    preview::set_text(&mut m, super::Text::Key("preview.loading"));
    m.preview_body = "raw 日本語 text".into();
    m.say_key("paste.nothing");
    let identity = (
        m.preview_generation,
        m.preview_requested,
        m.preview_path.clone(),
    );
    let revision = m.locale_revision;
    f.write("language", "ja\n");
    assert!(m.refresh_frame_locale());
    assert_eq!(m.locale_revision, revision + 1);
    assert_eq!(m.prompts.i18n.language(), "ja");
    assert_eq!(m.preview_state.i18n.language(), "ja");
    assert_eq!(m.preview.last().unwrap(), "読み込み中");
    assert_eq!(m.message, "空です");
    assert!(m
        .properties
        .as_ref()
        .unwrap()
        .iter()
        .any(|(key, _)| key == "容量"));
    assert_eq!(m.preview_body, "raw 日本語 text");
    assert_eq!(
        (
            m.preview_generation,
            m.preview_requested,
            m.preview_path.clone()
        ),
        identity
    );
    assert!(!m.refresh_frame_locale());
}

#[test]
fn tui_words_preview_join_peeked_retains_semantics_and_rejects_stale_path() {
    let f = fixture();
    let mut m = model(&f);
    m.rows.get_mut(&0).unwrap().directory = true;
    m.preview_children = m.current_path();
    let path = m.current_path().unwrap().to_string_lossy().into_owned();
    let reply = Json::Obj(vec![
        ("t".into(), wire::word("peeked")),
        ("path".into(), wire::word(&path)),
        ("failed".into(), Json::Bool(true)),
    ]);
    let (mut backend, reader) = wire::echo_wire();
    m.receive(reply, &mut backend).unwrap();
    assert_eq!(m.preview_state.refusal, Some("pane.unread"));
    assert_eq!(m.preview_refusal, "Cannot read folder");
    f.write("language", "ja\n");
    assert!(m.refresh_frame_locale());
    assert_eq!(m.preview_refusal, "フォルダを読めません");
    m.receive(
        jsondoc::parse(r#"{"t":"peeked","path":"/stale","failed":false}"#).unwrap(),
        &mut backend,
    )
    .unwrap();
    assert_eq!(m.preview_state.refusal, Some("pane.unread"));
    wire::finish(backend, reader);
}

#[test]
fn tui_words_preview_join_keycap_change_refreshes_held_load_hint() {
    let f = fixture();
    let mut m = model(&f);
    let map = Map::parse("[[preset]]\nname=\"default\"\nmods=\"\"\nkey=\"[\"\naction=\"loadPreview\"\ncontext=\"listing\"\n");
    m.hints = crate::tui::keymap::Hints::new(&map, "default");
    let hint = m.hints.load_preview.clone();
    preview::set_text(
        &mut m,
        super::Text::Args("preview.loadHint", vec![hint.into()]),
    );
    let before = m.preview.last().unwrap().clone();
    let revision = m.locale_revision;
    let identity = (m.preview_generation, m.preview_path.clone());
    m.set_keycaps(
        flea::keys::display::KeycapSnapshot::new("fr", None, Some(0), 17).unwrap(),
        &map,
    )
    .unwrap();
    assert_ne!(m.preview.last().unwrap(), &before);
    assert!(m.preview.last().unwrap().contains(&m.hints.load_preview));
    assert_eq!(m.locale_revision, revision);
    assert_eq!((m.preview_generation, m.preview_path.clone()), identity);
}

#[test]
fn tui_words_preview_join_fault_keeps_key_and_external_data() {
    let f = fixture();
    let mut m = model(&f);
    m.fail_io(&preview::words::issue(
        "preview.unreadable",
        "legacy failure",
    ));
    assert_eq!(m.error, "Unreadable");
    f.write("language", "ja\n");
    assert!(m.refresh_frame_locale());
    assert_eq!(m.error, "読めません");
    m.dismiss_error();
    let diagnostic = "raw \u{202e} 日本語 diagnostic";
    m.fail_io(&std::io::Error::other(diagnostic));
    f.write("language", "en\n");
    assert!(m.refresh_frame_locale());
    assert_eq!(m.error, diagnostic);
}

#[test]
fn tui_words_preview_join_pointer_matches_translated_physical_play_cells() {
    for language in ["ja", "ar"] {
        let f = fixture();
        f.write("language", &format!("{language}\n"));
        let mut m = model(&f);
        let mut player = Player::resting(
            std::path::Path::new("/nonexistent/clip.mp4"),
            Protocol::None,
            false,
            (0, 0, 1, 1),
            (130, 30),
            (800, 600),
        );
        player.probe(10_000);
        player.force_starting_for_test();
        player.refresh_locale(&m.i18n);
        assert!(player.play_width() > 8);
        m.player = Some(player);
        let (row, start, columns) = picture::control_row(&m).unwrap();
        assert!(columns > 8);
        let physical = layout::visual_start(&m, start, columns) + 1 + 8;
        let mut key = Key::named("Pointer", "");
        key.pointer = Some(Pointer {
            button: 0,
            x: physical,
            y: row,
            released: false,
            motion: false,
        });
        let (mut backend, reader) = wire::echo_wire();
        actions::key(&mut m, &key, &Map::load(), &mut backend).unwrap();
        wire::finish(backend, reader);
        let player = m.player.as_ref().unwrap();
        assert!(
            !player.paused,
            "{language}: translated play cell must toggle"
        );
        assert_eq!(player.control, 0);
        assert_eq!(player.resume, Some((0.0, false)));
    }
}

#[test]
fn tui_words_preview_join_empty_folder_keeps_semantic_label() {
    let f = fixture();
    let mut m = model(&f);
    m.rows.get_mut(&0).unwrap().directory = true;
    m.preview_children = m.current_path();
    let path = m.current_path().unwrap().to_string_lossy().into_owned();
    let reply = Json::Obj(vec![
        ("t".into(), wire::word("peeked")),
        ("path".into(), wire::word(&path)),
        ("n".into(), wire::number(0)),
    ]);
    let (mut backend, reader) = wire::echo_wire();
    m.receive(reply, &mut backend).unwrap();
    wire::finish(backend, reader);
    assert_eq!(m.preview.last().unwrap(), "Empty folder");
    f.write("language", "ja\n");
    assert!(m.refresh_frame_locale());
    assert_eq!(m.preview.last().unwrap(), "空のフォルダ");
}
