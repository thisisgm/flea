// Ported from tests/js/picker.js: the request, label, answer and URI cases, with every refusal kept.
use super::*;

// The shape tools/flea-portal writes for an OpenFile with two filters, taken from its request_for().
const ASKED: &str = r#"{"mode":"open","title":"Send to unraid","app":"","accept":"Send","multiple":true,
    "directory":false,"folder":"/home/gm/Pictures","file":"","name":"","files":[],
    "filters":[{"label":"Images","globs":["*.png","*.jpg"],"mimes":[]},{"label":"Any text","globs":[],"mimes":["text/plain"]}],
    "current":"Images"}"#;

fn caller(text: &str) -> Text {
    Text::Caller(text.into())
}

#[test]
fn the_portal_request_is_read_and_a_short_one_still_opens() {
    let req = request(ASKED);
    assert_eq!(req.mode, Mode::Open);
    assert_eq!(req.title, "Send to unraid");
    assert!(req.multiple);
    assert_eq!(req.folder, "/home/gm/Pictures");
    assert_eq!(req.filters.len(), 2);
    assert_eq!(req.filters[0].globs, ["*.png", "*.jpg"]);
    assert_eq!(req.filters[1].mimes, ["text/plain"]);
    let empty = request("not json");
    assert_eq!(
        empty.mode,
        Mode::Open,
        "a request that is not JSON still opens a window"
    );
    assert!(empty.filters.is_empty() && !empty.multiple && empty.hidden.is_none());
    assert_eq!(
        request(r#"{"mode":"nonsense"}"#).mode,
        Mode::Open,
        "an absent mode is never save"
    );
    assert_eq!(request(r#"{"mode":"savefiles"}"#).mode, Mode::SaveFiles);
    for text in [
        "",
        "null",
        "5",
        "[1]",
        r#"{"mode":7,"multiple":"true","filters":{}}"#,
    ] {
        let read = request(text);
        assert_eq!(
            (read.mode, read.multiple, read.filters.len()),
            (Mode::Open, false, 0),
            "{text}"
        );
    }
}

#[test]
fn a_field_reads_as_javascript_string_would() {
    let req = request(
        r#"{"title":5,"app":0,"accept":true,"name":false,"folder":null,"file":["a",1],"current":{}}"#,
    );
    assert_eq!(req.title, "5");
    assert_eq!(req.app, "");
    assert_eq!(req.accept, "true");
    assert_eq!(req.name, "");
    assert_eq!(req.folder, "");
    assert_eq!(req.file, "a,1");
    assert_eq!(req.current, "[object Object]");
    assert_eq!(
        request(r#"{"title":"a","title":"b"}"#).title,
        "b",
        "the last duplicate wins as in JSON.parse"
    );
    assert_eq!(request(r#"{"hidden":false}"#).hidden, Some(false));
    assert_eq!(
        request(r#"{"hidden":"yes"}"#).hidden,
        None,
        "visibility is only ever a boolean"
    );
    assert_eq!(request(r#"{"currentIndex":1.5}"#).current_index, -1);
}

#[test]
fn the_title_requester_and_accept_label_follow_the_caller() {
    let req = request(ASKED);
    assert_eq!(
        title(&req),
        caller("Send to unraid"),
        "the caller's title wins"
    );
    assert_eq!(title(&request("{}")), Text::Key("picker.portal.title.file"));
    assert_eq!(
        title(&request(r#"{"directory":true}"#)),
        Text::Key("picker.portal.title.folder")
    );
    assert_eq!(
        title(&request(r#"{"mode":"save"}"#)),
        Text::Key("picker.portal.title.save")
    );
    assert_eq!(
        title(&request(r#"{"mode":"savefiles"}"#)),
        Text::Key("picker.portal.title.save")
    );
    assert_eq!(requester(&req), None, "no app id draws no requester line");
    assert_eq!(
        requester(&request(r#"{"app":"a%1b"}"#)),
        Some("a%1b"),
        "the caller's text is kept whole"
    );
    assert_eq!(
        accept_label(&req, 3),
        AcceptLabel {
            base: caller("Send"),
            count: Some(3)
        }
    );
    assert_eq!(
        accept_label(&req, 1).count,
        None,
        "one file does not carry a count"
    );
    assert_eq!(
        accept_label(&request(r#"{"accept":"Se%2nd","multiple":true}"#), 3).base,
        caller("Se%2nd")
    );
    assert_eq!(
        accept_label(&request("{}"), 0).base,
        Text::Key("picker.portal.accept.open")
    );
    assert_eq!(
        accept_label(&request("{}"), 4).count,
        None,
        "a one-file request never counts"
    );
    assert_eq!(
        accept_label(&request(r#"{"directory":true}"#), 0).base,
        Text::Key("picker.portal.accept.folder")
    );
    assert_eq!(
        accept_label(&request(r#"{"mode":"savefiles"}"#), 0).base,
        Text::Key("picker.portal.accept.folder")
    );
    assert_eq!(
        accept_label(&request(r#"{"mode":"save"}"#), 0).base,
        Text::Key("picker.portal.accept.save")
    );
}

#[test]
fn the_hints_name_only_the_keys_a_mode_answers() {
    assert_eq!(
        hints(&request(ASKED), false),
        "picker.portal.hints.mark_files"
    );
    assert_eq!(hints(&request("{}"), false), "picker.portal.hints.open");
    assert_eq!(
        hints(&request(r#"{"mode":"save","multiple":true}"#), false),
        "picker.portal.hints.save"
    );
    assert_eq!(
        hints(&request(r#"{"multiple":true,"directory":true}"#), false),
        "picker.portal.hints.mark_folders"
    );
    assert_eq!(
        hints(&request(ASKED), true),
        "picker.portal.hints.unavailable"
    );
}

#[test]
fn the_current_filter_is_the_callers_rule_identity() {
    let req = request(ASKED);
    assert_eq!(current_chip(&req), 0);
    assert_eq!(
        current_chip(&request(r#"{"filters":[{"label":"A"}],"current":"Z"}"#)),
        0
    );
    assert_eq!(current_chip(&request("{}")), -1);
    let duplicate = r#"{"filters":[{"label":"A"},{"label":"A"}],"current":"A","currentIndex":1}"#;
    assert_eq!(
        current_chip(&request(duplicate)),
        1,
        "duplicate labels keep the caller's rule identity"
    );
    assert_eq!(
        current_chip(&request(
            r#"{"filters":[{"label":"A"},{"label":"B"}],"current":"B","currentIndex":9}"#
        )),
        1
    );
}

#[test]
fn the_start_folder_and_seeds_refuse_what_the_caller_cannot_name() {
    assert_eq!(
        start_folder(&request(r#"{"folder":"/srv"}"#), "/home/u"),
        "/srv"
    );
    assert_eq!(
        start_folder(&request(r#"{"folder":"flea:recent"}"#), "/home/u"),
        "/home/u"
    );
    assert_eq!(
        start_folder(&request(r#"{"folder":"rel/dir"}"#), "/home/u"),
        "/home/u"
    );
    assert_eq!(initial_save_name(&request(r#"{"name":"a.txt"}"#)), "a.txt");
    // tools/flea-portal reads current_name verbatim, so the window is the thing that refuses a traversal.
    let traversal = request(r#"{"name":"../../pwn"}"#);
    assert_eq!(
        traversal.name, "../../pwn",
        "a traversal survives the request unchanged"
    );
    assert_eq!(initial_save_name(&traversal), "");
    assert!(initial_hidden(&request(r#"{"hidden":true}"#), false));
    assert!(!initial_hidden(&request(r#"{"hidden":false}"#), true));
    assert!(
        initial_hidden(&request("{}"), true),
        "an absent value keeps the saved default"
    );
}

#[test]
fn the_save_name_stays_inside_the_folder_the_user_was_shown() {
    for name in ["ordinary.txt", ".bashrc", "a name with spaces"] {
        assert!(valid_name(name), "{name}");
    }
    for name in [
        "",
        ".",
        "..",
        "../escape",
        "/etc/passwd",
        "sub/child",
        "../../.config/autostart/pwn.desktop",
        "nul\0byte",
    ] {
        assert!(!valid_name(name), "{name:?}");
    }
}

#[test]
fn rows_join_under_their_location_and_recent_is_no_path() {
    assert!(!RECENT.starts_with('/'));
    assert!(is_recent(RECENT) && !is_recent("/home/gm"));
    assert_eq!(row_path("/home/gm", "a.png"), "/home/gm/a.png");
    assert_eq!(
        row_path(RECENT, "home/gm/Pictures/a.png"),
        "/home/gm/Pictures/a.png"
    );
    assert_eq!(row_path("/", "etc"), "/etc");
    assert!(
        directory(false, 0o120777, "folder"),
        "a symlinked folder stays navigable"
    );
    assert!(
        !directory(false, 0o100644, "folder"),
        "a regular file with a folder icon stays a file"
    );
    assert!(directory(true, 0o040755, "text"));
}

#[test]
fn a_picked_path_leaves_as_one_encoded_file_uri() {
    assert_eq!(file_uri("/home/gm/a.txt"), "file:///home/gm/a.txt");
    assert_eq!(
        file_uri("/home/gm/my file.txt"),
        "file:///home/gm/my%20file.txt"
    );
    assert_eq!(file_uri("/home/gm/a#b.txt"), "file:///home/gm/a%23b.txt");
    assert_eq!(file_uri("/q?x"), "file:///q%3Fx");
    assert_eq!(file_uri("/100%"), "file:///100%25");
    assert_eq!(file_uri("/caf\u{e9}"), "file:///caf%C3%A9");
    assert_eq!(
        file_uri("/a-_.!~*'();,:@&=+$"),
        "file:///a-_.!~*'();,:@&=+$"
    );
    assert_eq!(file_uri("/x/a #.png"), "file:///x/a%20%23.png");
}

#[test]
fn a_pick_answers_with_uris_and_a_refusal_with_none() {
    let uris = [file_uri("/home/gm/a.txt")];
    assert_eq!(
        reply(RESPONSE_OK, &uris, None),
        r#"{"response":0,"uris":["file:///home/gm/a.txt"]}"#
    );
    assert_eq!(
        reply(RESPONSE_OK, &uris, Some(1)),
        r#"{"response":0,"uris":["file:///home/gm/a.txt"],"filter":1}"#
    );
    assert_eq!(
        reply(RESPONSE_OK, &uris, Some(-1)),
        r#"{"response":0,"uris":["file:///home/gm/a.txt"],"filter":-1}"#
    );
    assert_eq!(
        reply(RESPONSE_CANCELLED, &uris, Some(1)),
        r#"{"response":1}"#
    );
}

#[test]
fn the_tab_ring_skips_hidden_and_disabled_stops() {
    let stop = |item: &'static str| Stop {
        item,
        visible: true,
        enabled: true,
        tab: true,
    };
    let walk = |filter: bool, save: bool, back: bool, answer: bool, view: &'static str| {
        let save = if save { vec![stop("Filename")] } else { vec![] };
        let answers = [
            Stop {
                visible: filter,
                ..stop("Filter")
            },
            stop("Cancel"),
            Stop {
                enabled: answer,
                ..stop("Answer")
            },
        ];
        let chrome = [
            Stop {
                enabled: back,
                ..stop("Back")
            },
            stop("Up"),
        ];
        tab_order(&chrome, stop("Rail"), stop(view), &save, &answers).join(",")
    };
    assert_eq!(
        walk(true, false, true, true, "List"),
        "Back,Up,Rail,List,Filter,Cancel,Answer"
    );
    assert_eq!(
        walk(false, false, true, true, "List"),
        "Back,Up,Rail,List,Cancel,Answer"
    );
    assert_eq!(
        walk(true, true, true, true, "List"),
        "Back,Up,Rail,List,Filename,Filter,Cancel,Answer"
    );
    assert_eq!(
        walk(false, true, true, true, "List"),
        "Back,Up,Rail,List,Filename,Cancel,Answer"
    );
    assert_eq!(
        walk(true, false, false, true, "List"),
        "Up,Rail,List,Filter,Cancel,Answer"
    );
    assert_eq!(
        walk(true, false, true, false, "List"),
        "Back,Up,Rail,List,Filter,Cancel"
    );
    assert_eq!(
        walk(true, true, true, true, "Grid"),
        "Back,Up,Rail,Grid,Filename,Filter,Cancel,Answer"
    );
    assert!(!tab_order(
        &[],
        Stop {
            tab: false,
            ..stop("Rail")
        },
        stop("List"),
        &[],
        &[]
    )
    .contains(&"Rail"));
}

#[test]
fn a_step_wraps_and_no_origin_lands_on_cancel() {
    let order = ["Back", "Rail", "List", "Cancel", "Answer"];
    assert_eq!(
        step_focus(&order, Some("Answer"), false, "Cancel"),
        Some("Back")
    );
    assert_eq!(
        step_focus(&order, Some("Back"), true, "Cancel"),
        Some("Answer")
    );
    assert_eq!(
        step_focus(&order, Some("Rail"), false, "Cancel"),
        Some("List")
    );
    assert_eq!(step_focus(&order, None, false, "Cancel"), Some("Cancel"));
    assert_eq!(
        step_focus(&["Up", "List"], None, true, "Cancel"),
        Some("Up"),
        "without Cancel the first stop"
    );
    assert_eq!(
        step_focus(&order, Some("Gone"), false, "Cancel"),
        Some("Back"),
        "a stop no longer in the ring steps from -1"
    );
    assert_eq!(
        step_focus(&order, Some("Gone"), true, "Cancel"),
        Some("Cancel")
    );
    assert_eq!(step_focus::<&str>(&[], None, false, "Cancel"), None);
}
