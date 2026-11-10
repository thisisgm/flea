//! Native menu choices must query fresh providers before file validation and cannot outlive dismissal.
use super::*;
use crate::g2::{
    context_menus::menu::Outcome,
    send_share::{seams::Wire, ProviderSend},
};
use flea::jsondoc::Json;
use flea::ui_logic_context_menus::entry::Brand;

const READY: &str = r#"{"BackendState":"Running","Self":{"Capabilities":["https://tailscale.com/cap/file-sharing"]},"Peer":{"old":{"HostName":"Old","DNSName":"old.invalid.","Online":true,"TaildropTarget":1}}}"#;

// The stage rewrites the Tailscale row to its plugin-send action (g2/plugins/menu_send.rs).
const TAILSCALE: &str = "plugin-send:flea.tailscale";

#[path = "activation_peer_tests.rs"]
mod peer_tests;

struct NoWire;
impl Wire for NoWire {
    fn send(&self, _: String) -> bool {
        true
    }
}

fn warm(
    cx: &mut TestAppContext,
) -> (
    Entity<Shell>,
    &mut VisualTestContext,
    impl FnMut() -> Option<String>,
    u64,
) {
    warm_selection(cx, 0, Vec::new())
}

fn warm_at(
    cx: &mut TestAppContext,
    outside: bool,
) -> (
    Entity<Shell>,
    &mut VisualTestContext,
    impl FnMut() -> Option<String>,
    u64,
) {
    warm_selection(
        cx,
        usize::from(outside),
        if outside { vec![0] } else { vec![0, 1] },
    )
}

fn warm_selection(
    cx: &mut TestAppContext,
    cursor: usize,
    selected: Vec<usize>,
) -> (
    Entity<Shell>,
    &mut VisualTestContext,
    impl FnMut() -> Option<String>,
    u64,
) {
    let total = if cursor > 0 || selected.len() > 1 {
        2
    } else {
        1
    };
    let (backend, mut lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| {
        let mut shell = shell_with(backend, cx);
        shell.launch.settings.data =
            flea::jsondoc::parse(r#"{"menu":{"hidden":["runScript","actions"]}}"#).unwrap();
        shell.panes[0].as_mut().unwrap().pane.held = Rc::new(crate::list::pane::Held {
            listing: 2,
            total,
            start: 0,
            rows: [
                RowFacts {
                    name: "notes.txt".into(),
                    mode: 0o100644,
                    ..Default::default()
                },
                RowFacts {
                    name: "outside.txt".into(),
                    mode: 0o100644,
                    ..Default::default()
                },
            ]
            .into_iter()
            .take(total)
            .collect(),
        });
        shell.panes[0].as_mut().unwrap().selected = selected;
        shell
    });
    let capture = open_at(&shell, cx, 0, cursor, &mut lines);
    let id = capture.id;
    feed(
        &shell,
        cx,
        0,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[32768]}}"#),
    );
    feed(
        &shell,
        cx,
        0,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[]}}"#
        ),
    );
    shell.update_in(cx, |shell, window, cx| {
        let mut sender = ProviderSend::default();
        // A prior menu's completed status supplies the cached peer, without starting a process in this fixture.
        sender.menu_opened(
            &NoWire,
            &flea::ui_logic_send_share::providers::Facts {
                tailscale_command: "/fixture/tailscale".into(),
                send_command: "/fixture/send".into(),
                dropbox_command: "/fixture/dropbox".into(),
                dropbox_ready: true,
                ..Default::default()
            },
            None,
            &[],
            0,
        );
        sender.taildrop_done(1, 0, READY, "", &shell.locale.catalog);
        sender.dropbox_done(1, 0, "Up to date", "", &shell.locale.catalog);
        shell.menu.providers.sides[0] = sender;
        shell.menu.providers.account[0] = flea::ui_logic_send_share::dropbox::account(
            r#"{"personal":{"path":"/fixture/Dropbox"}}"#,
            "",
        );
        shell.menu.providers.destination[0] = Some(true);
        shell.refresh_menu(window, cx);
        assert!(flea::ui_logic_context_menus::refresh::validate_choice(
            &shell.menu_choice_entries(TAILSCALE, "old"),
            TAILSCALE,
            "old",
            false
        )
        .is_ok());
        assert!(flea::ui_logic_context_menus::refresh::validate_choice(
            &shell.menu_choice_entries("dropbox", ""),
            "dropbox",
            "",
            false,
        )
        .is_ok());
    });
    drain(&mut lines);
    (shell, cx, lines, id)
}

fn choose(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    shell.update_in(cx, |shell, window, cx| {
        shell.menu_outcome(
            Outcome::Fire {
                action: TAILSCALE.into(),
                leaf: "old".into(),
            },
            window,
            cx,
        )
    });
}

fn unavailable(id: u64) -> String {
    format!(r#"{{"t":"formats","id":{id},"archive":[],"providers":{{}}}}"#)
}

fn pending(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    super::state_tests::state(shell, cx)
        .get("pendingActivation")
        .and_then(Json::as_bool)
        .unwrap()
}

#[gpui::test]
fn uish_misc_provider_choice_refreshes_once_and_refuses_a_disappeared_peer(
    cx: &mut TestAppContext,
) {
    let (shell, cx, mut lines, id) = warm(cx);
    choose(&shell, cx);
    let asked = drain(&mut lines);
    assert_eq!(with(&asked, "\"c\":\"formats\"").len(), 1, "{asked:?}");
    assert!(with(&asked, "\"op\":\"validate\"").is_empty());
    assert!(pending(&shell, cx));
    choose(&shell, cx);
    assert!(
        with(&drain(&mut lines), "\"c\":\"formats\"").is_empty(),
        "repeated activation must share its fresh request"
    );
    feed(&shell, cx, 0, &unavailable(id + 1));
    assert!(
        pending(&shell, cx),
        "a stale answer cannot validate this menu"
    );
    feed(&shell, cx, 0, &unavailable(id));
    assert!(!pending(&shell, cx));
    assert!(shell.read_with(cx, |shell, _| shell.menu.captured.is_none()));
    let after = drain(&mut lines);
    assert!(with(&after, "\"op\":\"validate\"").is_empty(), "{after:?}");
    assert!(with(&after, "\"c\":\"send\"").is_empty());
}

#[gpui::test]
fn uish_misc_provider_choice_waits_for_old_metadata_then_asks_again(cx: &mut TestAppContext) {
    let (shell, cx, mut lines, id) = warm(cx);
    shell.update_in(cx, |shell, window, cx| {
        shell.providers_menu_opened(0, id, window, cx)
    });
    drain(&mut lines);
    choose(&shell, cx);
    assert!(with(&drain(&mut lines), "\"c\":\"formats\"").is_empty());
    feed(&shell, cx, 0, &facts(id));
    let fresh = drain(&mut lines);
    assert_eq!(
        with(&fresh, "\"c\":\"formats\"").len(),
        1,
        "the old metadata answer is not the activation's fresh request: {fresh:?}"
    );
    assert!(with(&fresh, "\"op\":\"validate\"").is_empty());
    assert!(pending(&shell, cx));
    feed(&shell, cx, 0, &unavailable(id));
    assert!(!pending(&shell, cx));
}

#[gpui::test]
fn uish_misc_provider_choice_cannot_resume_after_dismissal(cx: &mut TestAppContext) {
    let (shell, cx, mut lines, id) = warm(cx);
    choose(&shell, cx);
    drain(&mut lines);
    shell.update_in(cx, |shell, window, cx| shell.retire_menu(window, cx));
    drain(&mut lines);
    feed(&shell, cx, 0, &unavailable(id));
    assert!(!pending(&shell, cx));
    assert!(shell.read_with(cx, |shell, _| shell.menu.captured.is_none()));
    assert!(with(&drain(&mut lines), "\"op\":\"validate\"").is_empty());
}

#[gpui::test]
fn uish_misc_provider_activation_keeps_original_dropbox_destination(cx: &mut TestAppContext) {
    let (shell, cx, mut lines, id) = warm(cx);
    shell.update_in(cx, |shell, window, cx| {
        shell.menu_outcome(
            Outcome::Fire {
                action: "dropbox".into(),
                leaf: String::new(),
            },
            window,
            cx,
        )
    });
    let asked = drain(&mut lines);
    assert_eq!(with(&asked, "\"c\":\"formats\"").len(), 1);
    feed(
        &shell,
        cx,
        0,
        &format!(
            r#"{{"t":"formats","id":{id},"archive":[],"providers":{{"dropbox":{{"installed":true,"command":"/fixture/dropbox"}},"dropboxInfo":"{{\"personal\":{{\"path\":\"/fixture/Dropbox\"}}}}"}}}}"#
        ),
    );
    let after = drain(&mut lines);
    assert!(with(&after, "\"op\":\"providerDestination\"").is_empty(),
        "activation must validate against the original destination, never recapture a replacement: {after:?}");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.menu.providers.destination[0]),
        Some(true)
    );
}

#[gpui::test]
fn uish_misc_unusable_provider_rows_draw_disabled_and_red(cx: &mut TestAppContext) {
    let (shell, cx, _lines, _id) = warm(cx);
    let facts = flea::ui_logic_send_share::providers::parse_facts(r#"{"taildrop":{"installed":true,"command":"","reason":"not executable"},"dropbox":{"installed":true,"command":"","reason":"not executable"},"dropboxInfo":"{}"}"#).unwrap();
    let rows = shell.update_in(cx, |shell, window, cx| {
        let mut sender = ProviderSend::default();
        sender.menu_opened(&NoWire, &facts, None, &[], 0);
        shell.menu.providers.sides[0] = sender;
        shell.refresh_menu(window, cx);
        shell.menu_entries()
    });
    for brand in [Brand::Tailscale, Brand::Dropbox] {
        let row = rows.iter().find(|row| row.brand == Some(brand));
        // Errored pins the cannot-answer branch; a row still reading is disabled but not red.
        assert!(
            row.is_some_and(|row| row.disabled && row.errored),
            "{brand:?} row: {row:?}"
        );
    }
}
