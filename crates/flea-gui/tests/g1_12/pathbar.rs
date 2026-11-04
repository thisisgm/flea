//! tests/js/pathbar.js, case for case: what a typed line resolves to, Tab's completion and what the bar says.
use super::HOME;
use flea::{
    keys::{lookup, Context, Frontend, Input, CONTROL, SHIFT},
    protocol::{Reply, RowFacts},
};
use flea_gui::{
    navigation::{
        folder_state::{Folders, Writer},
        Command, Navigator,
    },
    pane::{Kind, Pane},
    path::{
        complete, completion_dir, completion_message, failed, normalize, peek_line, refused,
        request_key, resolve, should_navigate, wants_hidden, Completion,
    },
};
use std::{cell::RefCell, rc::Rc};

#[track_caller]
fn check<T: PartialEq + std::fmt::Debug>(line: u32, got: T, want: T) {
    super::js("pathbar.js", line, got, want);
}

fn said(before: &str, after: &Completion, dir: &str) -> String {
    completion_message(before, after, dir)
        .map(|said| said.text(&crate::test_catalogs::english()))
        .unwrap_or_default()
}

// Nav.pathFailed over showingPane(path, state, inFlight, trashOpened, searchMode).
fn showing(kind: Kind, in_flight: bool, trash: bool, search: bool) -> bool {
    failed(kind, in_flight, search, trash)
}

#[test]
fn completion_folds_only_when_nothing_matched_and_never_shrinks_the_line() {
    let folded = complete("~/re", &["README", "ReadMe.bak"]);
    check(50, folded.text.as_str(), "~/re");
    check(51, folded.matches, 2);
    check(
        52,
        said("~/re", &folded, HOME),
        "2 items start with that".into(),
    );
    check(56, complete("~/de", &["Desktop"]).text, "~/Desktop/".into());
}

#[test]
fn a_typed_line_resolves_the_way_the_pane_would_list_it() {
    check(59, resolve("/etc", HOME, HOME), "/etc".into());
    check(60, resolve("~", HOME, HOME), HOME.into());
    check(
        61,
        resolve("~/Downloads", HOME, HOME),
        "/home/gm/Downloads".into(),
    );
    check(
        62,
        resolve("Downloads", HOME, HOME),
        "/home/gm/Downloads".into(),
    );
    check(64, resolve("src", "/usr", HOME), "/usr/src".into());
    check(68, resolve("~", "/etc", ""), "/etc/~".into());
    check(72, resolve("", HOME, HOME), String::new());
    check(73, resolve("   ", HOME, HOME), String::new());
    check(77, resolve("/etc/", HOME, HOME), "/etc".into());
    check(78, resolve("/etc//apt", HOME, HOME), "/etc/apt".into());
    check(79, resolve("/", HOME, HOME), "/".into());
    check(80, normalize("//"), "/".into());
    check(84, resolve("/etc/./apt", HOME, HOME), "/etc/apt".into());
    check(85, resolve("/etc/apt/..", HOME, HOME), "/etc".into());
    check(
        86,
        resolve("../Music", "/home/gm/Pictures", HOME),
        "/home/gm/Music".into(),
    );
    check(88, resolve("/../../..", HOME, HOME), "/".into());
}

#[test]
fn file_uris_decode_and_only_this_machine_is_a_path() {
    check(
        92,
        resolve("file:///etc/apt", HOME, HOME),
        "/etc/apt".into(),
    );
    check(
        93,
        resolve("file:///home/gm/My%20Files", HOME, HOME),
        "/home/gm/My Files".into(),
    );
    check(
        96,
        resolve("file:///tmp/100%", HOME, HOME),
        "/tmp/100%".into(),
    );
    check(
        102,
        resolve("file://localhost/etc", HOME, HOME),
        "/etc".into(),
    );
    check(
        104,
        resolve("file://LOCALHOST/etc/apt", HOME, HOME),
        "/etc/apt".into(),
    );
    check(106, resolve("file://localhost", HOME, HOME), "/".into());
    check(
        110,
        resolve("file://otherbox/etc", HOME, HOME),
        String::new(),
    );
    check(112, refused("file://otherbox/etc"), true);
    check(113, refused("   "), false);
    check(114, refused("/etc"), false);
    check(
        246,
        resolve("smb://servername.lan", HOME, HOME),
        "smb://servername.lan".into(),
    );
    check(
        247,
        resolve("smb://dan@192.168.1.75/test", HOME, HOME),
        "smb://dan@192.168.1.75/test".into(),
    );
}

#[test]
fn only_a_settled_error_or_locked_listing_retries_the_same_path() {
    check(117, showing(Kind::Error, false, false, false), true);
    check(118, showing(Kind::Locked, false, false, false), true);
    check(119, showing(Kind::Ready, false, false, false), false);
    check(120, showing(Kind::Empty, false, false, false), false);
    check(121, showing(Kind::Error, true, false, false), false);
    check(122, showing(Kind::Error, false, true, false), false);
    check(123, showing(Kind::Error, false, false, true), false);
    check(
        124,
        should_navigate(HOME, HOME, showing(Kind::Error, false, false, false)),
        true,
    );
    check(
        126,
        should_navigate(HOME, HOME, showing(Kind::Locked, false, false, false)),
        true,
    );
    check(
        128,
        should_navigate(HOME, HOME, showing(Kind::Ready, false, false, false)),
        false,
    );
    check(
        130,
        should_navigate("/etc", "/etc", showing(Kind::Empty, false, false, false)),
        false,
    );
    check(
        132,
        should_navigate("Trash", "Trash", showing(Kind::Error, false, true, false)),
        false,
    );
    check(
        134,
        should_navigate(HOME, HOME, showing(Kind::Error, false, false, true)),
        false,
    );
    check(
        136,
        should_navigate(HOME, HOME, showing(Kind::Ready, true, false, false)),
        false,
    );
    check(
        138,
        should_navigate("/etc", HOME, true) && should_navigate("/etc", HOME, false),
        true,
    );
    check(
        140,
        should_navigate("/etc", HOME, showing(Kind::Ready, true, false, false)),
        true,
    );
    check(
        142,
        should_navigate("", HOME, true) || should_navigate("", HOME, false),
        false,
    );
}

fn settled(path: &str) -> (Pane, Navigator) {
    let folders = Folders::from_settings(
        &flea::jsondoc::parse("{\"hiddenLast\":true}").unwrap(),
        Vec::new(),
    );
    let mut nav = Navigator::new(
        Rc::new(RefCell::new(folders)),
        Rc::new(RefCell::new(Writer::default())),
    );
    let mut pane = Pane::new(path.into(), 620.);
    let now = pane.now();
    pane.receive_at(
        Reply::Listed {
            path: path.into(),
            total: 3,
        },
        now,
    )
    .unwrap();
    let rows = vec![
        RowFacts {
            name: "a".into(),
            ..RowFacts::default()
        };
        3
    ];
    pane.receive_at(
        Reply::Rows {
            start: 0,
            rows,
            listing: 1,
        },
        now,
    )
    .unwrap();
    nav.after_reply(&mut pane, path, &mut Vec::new());
    (pane, nav)
}

#[test]
fn a_path_entered_in_flight_supersedes_the_listing_out_and_the_shell_judges_real_panes_alike() {
    let (mut pane, mut nav) = settled(HOME);
    assert_eq!(
        failed(pane.state.kind, pane.busy(), pane.state.search, false),
        nav.path_failed(&pane)
    );
    assert_eq!(nav.open(&mut pane, "/f/x"), Ok(true));
    nav.take();
    assert!(pane.busy());
    assert_eq!(
        failed(pane.state.kind, pane.busy(), pane.state.search, false),
        nav.path_failed(&pane)
    );
    check(154, nav.open(&mut pane, "/etc"), Ok(true));
    let pending = pane
        .swap
        .pending
        .as_ref()
        .map(|pending| (pending.path.as_str(), pending.options.supersede));
    check(155, pending, Some(("/etc", true)));
    let lists: Vec<String> = nav
        .take()
        .into_iter()
        .filter_map(|command| match command {
            Command::List(listing) => Some(listing.path),
            _ => None,
        })
        .collect();
    check(155, lists, vec!["/etc".to_owned()]);
    let now = pane.now();
    assert!(pane.fail("scan", "/etc", "Permission denied", 0o40700, now));
    assert_eq!(
        failed(pane.state.kind, pane.busy(), pane.state.search, false),
        nav.path_failed(&pane)
    );
    assert!(
        nav.path_failed(&pane),
        "a refused listing is the retry case"
    );
}

#[test]
fn tab_reads_the_head_of_the_line_and_asks_by_directory_and_flag() {
    check(158, completion_dir("/etc/ap", HOME, HOME), "/etc".into());
    check(160, completion_dir("Dow", HOME, HOME), HOME.into());
    check(
        162,
        completion_dir("/etc/apt/", HOME, HOME),
        "/etc/apt".into(),
    );
    check(164, completion_dir("~/Do", HOME, HOME), HOME.into());
    check(168, wants_hidden("~/.co", false), true);
    check(169, wants_hidden("~/Do", false), false);
    check(170, wants_hidden("~/Do", true), true);
    check(176, request_key(HOME, true), request_key(HOME, true));
    check(
        178,
        request_key(HOME, true) == request_key(HOME, false),
        false,
    );
    check(
        180,
        request_key("/etc", false) == request_key(HOME, false),
        false,
    );
    let pending = request_key(HOME, true);
    check(185, request_key(HOME, false) == pending, false);
    check(187, request_key(HOME, true) == pending, true);
}

#[test]
fn tab_grows_the_line_as_far_as_the_names_honestly_share() {
    let dirs = ["Desktop", "Documents", "Downloads", "Dropbox", "Music"];
    let one = complete("/home/gm/Mu", &["Music", "Pictures"]);
    check(193, one.text.as_str(), "/home/gm/Music/");
    check(194, one.matches, 1);
    let many = complete("/home/gm/D", &dirs);
    check(198, many.text.as_str(), "/home/gm/D");
    check(199, many.matches, 4);
    check(
        200,
        complete("/home/gm/Do", &dirs).text,
        "/home/gm/Do".into(),
    );
    check(
        202,
        complete("/home/gm/Doc", &dirs).text,
        "/home/gm/Documents/".into(),
    );
    check(207, complete("/srv/", &["www"]).text, "/srv/www/".into());
    check(
        210,
        complete("/home/gm/zz", &dirs).text,
        "/home/gm/zz".into(),
    );
    check(212, complete("/home/gm/zz", &dirs).matches, 0);
    check(
        217,
        complete("/home/gm/dow", &dirs).text,
        "/home/gm/Downloads/".into(),
    );
    check(
        218,
        complete("/home/gm/D", &["Dev", "dev"]).text,
        "/home/gm/Dev/".into(),
    );
    check(222, said("/home/gm/Mu", &one, HOME), String::new());
    check(
        224,
        said("/home/gm/zz", &complete("/home/gm/zz", &dirs), HOME),
        "Nothing in /home/gm starts with that".into(),
    );
    check(
        227,
        said("/home/gm/D", &many, HOME),
        "4 items start with that".into(),
    );
    let grouped = Completion {
        text: "/home/gm/D".into(),
        matches: 1204,
    };
    check(
        229,
        said("/home/gm/D", &grouped, HOME),
        "1,204 items start with that".into(),
    );
}

#[test]
fn colon_and_ctrl_l_name_the_path_bar_and_its_peek_opts_out_of_hidden_last() {
    let key = |key: &str, text: &str, modifiers: u8, context: Context| {
        lookup(
            "default",
            context,
            Frontend::Gui,
            Input {
                key,
                text,
                modifiers,
            },
        )
        .action()
    };
    check(237, key(":", ":", SHIFT, Context::Listing), "pathBar");
    check(237, key("L", "l", CONTROL, Context::Listing), "pathBar");
    check(
        250,
        peek_line(HOME, 40, true).contains("\"hiddenLast\":false"),
        true,
    );
    check(
        250,
        peek_line(HOME, 40, false),
        "{\"c\":\"peek\",\"path\":\"/home/gm\",\"first\":40,\"hidden\":false,\"hiddenLast\":false}"
            .into(),
    );
    // Every other read keeps the folder's own order: the listing still asks with the remembered hiddenLast.
    let (pane, mut nav) = settled(HOME);
    check(252, nav.listing(&pane, HOME, false).hidden_last, true);
}
