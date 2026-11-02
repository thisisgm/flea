use crate::backend::testdir::TestDir;
use crate::{
    jsondoc::{self, Json},
    tui::{
        action_key_tests::{drain, listing},
        actions,
        detail::{self, Column},
        editor::Editor,
        input::Key,
        keymap::Map,
        model::Row,
        parity17,
        wire::{self, count, text},
    },
};
fn parse(s: &str) -> Json {
    jsondoc::parse(s).unwrap()
}

#[test]
fn word_commands_keep_utf8_selections_suffixes_and_insert_only_typing() {
    let mut e = Editor::new("path", "é.猫 white".into(), "/".into());
    e.update(&Key::named("Left", "ctrlshift"));
    e.update(&Key::named("W", "ctrl"));
    assert_eq!(e.value, "é.猫 ");
    e.update(&Key::named("Backspace", "ctrl"));
    assert_eq!(e.value, "é.");
    e.update(&Key::named("Backspace", "ctrl"));
    assert_eq!(e.value, "é");
    e.update(&Key::named("Backspace", "ctrl"));
    assert_eq!(e.value, "");
    for c in "bfdwu".chars() {
        e.update(&Key::character(c, ""));
    }
    assert_eq!(e.value, "bfdwu");
    let root = TestDir::new("tui17-word-suffix");
    let path = root.file("alpha.beta.txt", "keep");
    let mut e = Editor::rename("alpha.beta.txt".into(), path, false).unwrap();
    e.update(&Key::named("Left", ""));
    e.update(&Key::named("Right", "ctrl"));
    e.update(&Key::named("K", "ctrl"));
    assert_eq!(e.value, "alpha.txt");
}

#[test]
fn copy_variants_keep_selected_names_uri_bytes_and_parent_fallback() {
    let paths = vec!["/x/a é.tar.gz".into(), "/x/.bashrc".into()];
    for (form, want) in [
        ("path", "/x/a é.tar.gz\n/x/.bashrc"),
        ("url", "file:///x/a%20%C3%A9.tar.gz\nfile:///x/.bashrc"),
        ("dirpath", "/x\n/x"),
        ("dirurl", "file:///x\nfile:///x"),
        ("filename", "a é.tar.gz\n.bashrc"),
        ("name_without_ext", "a é.tar\n.bashrc"),
    ] {
        assert_eq!(
            parity17::copy_text(&paths, form, std::path::Path::new("/empty")),
            want
        );
    }
    assert_eq!(
        parity17::copy_text(&[], "dirpath", std::path::Path::new("/empty")),
        "/empty"
    );
    assert_eq!(
        parity17::copy_text(&[], "dirurl", std::path::Path::new("/empty")),
        "file:///empty"
    );
}

#[test]
fn text_copy_publishes_plain_text_only_after_a_current_identity_reply() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    w.listing = Some(7);
    actions::act(&mut m, "copyUri", &mut w).unwrap();
    drain(&mut w);
    m.receive(
        parse(r#"{"t":"paths","paths":["/held/a b"],"listing":7}"#),
        &mut w,
    )
    .unwrap();
    let sent = drain(&mut w);
    let publish = sent.iter().find(|v| text(v, "c") == "clipSet").unwrap();
    assert_eq!(text(publish, "op"), "text");
    assert_eq!(
        publish.get("paths").unwrap().as_array().unwrap()[0].as_str(),
        Some("file:///held/a%20b")
    );
    assert!(m.clipboard.is_empty());
    actions::act(&mut m, "copyPath", &mut w).unwrap();
    drain(&mut w);
    w.listing = Some(8);
    m.receive(
        parse(r#"{"t":"paths","paths":["/stale"],"listing":7}"#),
        &mut w,
    )
    .unwrap();
    assert!(drain(&mut w).is_empty());
    wire::finish(w, reader);
}

#[test]
fn a_delayed_copy_failure_cannot_clear_a_newer_copy_request() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    w.listing = Some(7);
    actions::act(&mut m, "copyUri", &mut w).unwrap();
    drain(&mut w);
    let id = m.parity17.copy_id;
    m.receive(
        parse(&format!(
            r#"{{"t":"error","where":"paths","id":{},"listing":6,"msg":"old source changed"}}"#,
            id - 1
        )),
        &mut w,
    )
    .unwrap();
    assert_eq!(m.parity17.copy_text, Some("url"));
    assert!(m.error.is_empty());
    m.receive(
        parse(&format!(
            r#"{{"t":"paths","id":{id},"listing":7,"paths":["/listing/file"]}}"#
        )),
        &mut w,
    )
    .unwrap();
    assert!(drain(&mut w)
        .iter()
        .any(|reply| text(reply, "c") == "clipSet"));
    wire::finish(w, reader);
}

#[test]
fn reverse_find_wraps_then_next_advances_without_narrowing_the_listing() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    w.listing = Some(7);
    m.total = 5;
    m.cursor = 1;
    m.selected.insert(1);
    actions::act(&mut m, "findPrevious", &mut w).unwrap();
    actions::key(&mut m, &Key::character('a', ""), &Map::load(), &mut w).unwrap();
    let requests = drain(&mut w);
    let request = requests.iter().find(|v| text(v, "c") == "filter").unwrap();
    let id = count(request, "id");
    m.receive(
        parse(&format!(
            r#"{{"t":"found","id":{id},"query":"a","listing":7,"rows":[0,2,4]}}"#
        )),
        &mut w,
    )
    .unwrap();
    assert_eq!(m.cursor, 0);
    assert_eq!(m.total, 5);
    assert!(m.filter.is_empty() && m.selected.contains(&1));
    actions::key(&mut m, &Key::named("Return", ""), &Map::load(), &mut w).unwrap();
    drain(&mut w);
    actions::act(&mut m, "findNext", &mut w).unwrap();
    assert_eq!(m.cursor, 2);
    crate::tui::finder::next(&mut m, true, &mut w).unwrap();
    assert_eq!(m.cursor, 0);
    crate::tui::finder::next(&mut m, true, &mut w).unwrap();
    assert_eq!(m.cursor, 4);
    wire::finish(w, reader);
}

#[test]
fn direct_birth_and_off_modes_preserve_the_four_mode_cycle_and_properties() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    m.rows.insert(
        0,
        Row::parse(&parse(r#"{"n":"born","p":33188,"b":1700000000}"#), &[]),
    );
    actions::act(&mut m, "detailBirth", &mut w).unwrap();
    assert_eq!(m.detail.mode, Column::Birth);
    assert_eq!(
        detail::value(&m, 0, &m.rows[&0]),
        m.i18n.locale().format_date(1700000000).unwrap()
    );
    m.rows.get_mut(&0).unwrap().birth = None;
    assert_eq!(detail::value(&m, 0, &m.rows[&0]), "unavailable");
    actions::act(&mut m, "detailOff", &mut w).unwrap();
    assert!(detail::value(&m, 0, &m.rows[&0]).is_empty());
    for expected in [
        Column::Size,
        Column::Modified,
        Column::Mode,
        Column::Owner,
        Column::Size,
    ] {
        actions::act(&mut m, "detailNext", &mut w).unwrap();
        assert_eq!(m.detail.mode, expected);
    }
    let facts = crate::tui::render::cards::properties(&parse(r#"{"birth":null}"#), &m.i18n);
    assert!(facts.contains(&("Created".into(), "unavailable".into())));
    wire::finish(w, reader);
}

#[test]
fn follow_reveals_only_a_current_link_target_reply() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    w.listing = Some(7);
    m.rows.get_mut(&0).unwrap().link = "../target/name".into();
    actions::act(&mut m, "followLink", &mut w).unwrap();
    let requests = drain(&mut w);
    let request = requests
        .iter()
        .find(|v| text(v, "c") == "linktarget")
        .unwrap();
    assert_eq!(text(request, "target"), "../target/name");
    let id = count(request, "id");
    m.receive(parse(&format!(r#"{{"t":"linktarget","id":{},"path":"/listing/folder","directory":"/target","name":"name"}}"#,id+1)),&mut w).unwrap();
    assert!(m.pending.is_none());
    m.receive(parse(&format!(r#"{{"t":"linktarget","id":{id},"path":"/listing/folder","directory":"/target","name":"name"}}"#)),&mut w).unwrap();
    assert_eq!(m.pending.as_deref(), Some(std::path::Path::new("/target")));
    assert_eq!(
        m.restore_path.as_deref(),
        Some(std::path::Path::new("/target/name"))
    );
    wire::finish(w, reader);
}

#[test]
fn new_sorts_carry_the_cursor_anchor_and_clear_only_the_old_numbering() {
    for (action, by, reverse) in [
        ("sortBirth", "btime", false),
        ("sortBirthReverse", "btime", true),
        ("sortExtension", "extension", false),
        ("sortRandom", "random", false),
    ] {
        let (mut w, reader) = wire::echo_wire();
        let mut m = listing(&mut w, "yazi");
        m.cursor = 1;
        w.listing = Some(7);
        actions::act(&mut m, action, &mut w).unwrap();
        let sent = drain(&mut w);
        let request = sent.iter().find(|v| text(v, "c") == "sort").unwrap();
        assert_eq!(text(request, "by"), by);
        assert_eq!(request.get("desc").unwrap().as_bool(), Some(reverse));
        assert_eq!(text(request, "anchor"), "/listing/file");
        assert_eq!(
            m.sort_anchor.as_deref(),
            Some(std::path::Path::new("/listing/file"))
        );
        assert!(m.rows.is_empty());
        m.receive(
            parse(r#"{"t":"listed","path":"/listing","n":2,"listing":8,"anchor":"/listing/file","anchorIndex":0}"#),
            &mut w,
        )
        .unwrap();
        assert_eq!(m.cursor, 0);
        assert_eq!(w.listing, Some(8));
        wire::finish(w, reader);
    }
}

#[test]
fn only_the_folder_preview_requests_peek_metadata_and_tagged_copies_do_not_choose_files() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    m.preview_visible = true;
    crate::tui::preview::load(&mut m, true);
    crate::tui::preview::request(&mut m, &mut w).unwrap();
    let sent = drain(&mut w);
    let peek = sent.iter().find(|v| text(v, "c") == "peek").unwrap();
    assert_eq!(peek.get("details").unwrap().as_bool(), Some(true));
    actions::act(&mut m, "copyPath", &mut w).unwrap();
    drain(&mut w);
    m.choose_pending = true;
    m.choose_count = 1;
    let id = m.parity17.copy_id;
    m.receive(
        parse(&format!(r#"{{"t":"paths","id":{id},"paths":["/copy"]}}"#)),
        &mut w,
    )
    .unwrap();
    assert!(m.choose_pending && m.chosen.is_none());
    m.receive(parse(r#"{"t":"paths","paths":["/chosen"]}"#), &mut w)
        .unwrap();
    assert_eq!(m.chosen, Some(vec!["/chosen".into()]));
    wire::finish(w, reader);
}

#[test]
fn a_real_copy_holds_sources_through_collision_confirmation_and_chooser_paths_stay_separate() {
    let root = TestDir::new("tui17-copy-link");
    let source = root.file("source", "held");
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    w.listing = Some(7);
    actions::act(&mut m, "copy", &mut w).unwrap();
    let sent = drain(&mut w);
    let request = sent.iter().find(|v| text(v, "c") == "paths").unwrap();
    assert_eq!(request.get("hold").unwrap().as_bool(), Some(true));
    let id = count(request, "id");
    assert!(id > 0);
    let paths = vec![source.to_string_lossy().into_owned()];
    let held = crate::backend::linkheld::capture(&paths).unwrap();
    let reply = Json::Obj(vec![
        ("t".into(), wire::word("paths")),
        ("id".into(), wire::number(id)),
        ("listing".into(), wire::number(7)),
        (
            "paths".into(),
            Json::Arr(paths.iter().map(|p| wire::word(p)).collect()),
        ),
        ("held".into(), crate::backend::linkheld::encode(&held)),
    ]);
    m.receive(reply, &mut w).unwrap();
    assert!(!m.pending_clipboard);
    assert_eq!(m.parity17.copied.len(), 1);
    actions::act(&mut m, "pasteAbsoluteLink", &mut w).unwrap();
    let sent = drain(&mut w);
    let collision = sent.iter().find(|v| text(v, "c") == "collisions").unwrap();
    let id = count(collision, "id");
    m.receive(
        parse(&format!(r#"{{"t":"collisions","id":{id},"total":0}}"#)),
        &mut w,
    )
    .unwrap();
    let sent = drain(&mut w);
    let link = sent.iter().find(|v| text(v, "c") == "link").unwrap();
    assert_eq!(text(link, "op"), "absolute");
    assert_eq!(
        crate::backend::linkheld::decode(link.get("held").unwrap())
            .unwrap()
            .len(),
        1
    );
    m.choosing = true;
    m.cursor = 1;
    actions::act(&mut m, "open", &mut w).unwrap();
    let sent = drain(&mut w);
    let chooser = sent.iter().find(|v| text(v, "c") == "paths").unwrap();
    assert!(chooser.get("hold").is_none() && chooser.get("id").is_none());
    wire::finish(w, reader);
}

// A folder's listed line carries no listing stamp, so the backend's stamped Copy reply must still land: yazi y, then Copied 1 item.
#[test]
fn tui_fix_yazi_copy_y_copies_the_cursor_row_in_a_folder() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    assert_eq!(w.listing, None);
    actions::key(&mut m, &Key::character('j', ""), &Map::load(), &mut w).unwrap();
    actions::key(&mut m, &Key::character('y', ""), &Map::load(), &mut w).unwrap();
    let sent = drain(&mut w);
    let ask = sent.iter().find(|v| text(v, "c") == "paths").unwrap();
    assert_eq!(ask.get("hold").and_then(Json::as_bool), Some(true));
    let id = count(ask, "id");
    m.receive(
        parse(&format!(
            r#"{{"t":"paths","paths":["/listing/file"],"id":{id},"held":[],"listing":3}}"#
        )),
        &mut w,
    )
    .unwrap();
    assert_eq!(m.clipboard, vec!["/listing/file".to_string()]);
    assert!(!m.pending_clipboard);
    assert_eq!(m.message, "Copied 1 item");
    // A reply that lands after the folder changed is still dropped, so a stale row never fills the clipboard.
    actions::key(&mut m, &Key::character('y', ""), &Map::load(), &mut w).unwrap();
    let id = count(
        drain(&mut w)
            .iter()
            .find(|v| text(v, "c") == "paths")
            .unwrap(),
        "id",
    );
    m.pending = Some("/elsewhere".into());
    m.receive(
        parse(&format!(
            r#"{{"t":"paths","paths":["/listing/folder"],"id":{id},"held":[],"listing":4}}"#
        )),
        &mut w,
    )
    .unwrap();
    assert_eq!(m.clipboard, vec!["/listing/file".to_string()]);
    assert!(!m.pending_clipboard);
    wire::finish(w, reader);
}
