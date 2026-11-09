use super::*;
use crate::g2::send_share::seams::Wire;
use flea::ui_logic_send_share::providers::{Facts, Role};

struct Pipe;
impl Wire for Pipe {
    fn send(&self, _: String) -> bool {
        true
    }
}

fn ready_builtins(fixture: &Fixture, cx: &mut TestAppContext) {
    update(cx, fixture.handle, |shell, window, cx| {
        let states = shell.declared_provider_states();
        let sender = &mut shell.menu.providers.sides[0];
        sender.menu_opened(
            &Pipe,
            &Facts {
                tailscale_command: "true".into(),
                send_command: "true".into(),
                localsend_command: "true".into(),
                dropbox_command: "true".into(),
                dropbox_ready: true,
                ..Default::default()
            },
            Some(&states),
            &[],
            0,
        );
        sender.line(r#"{"t":"localsendpeers","id":1,"peers":[{"name":"Receiver","address":"10.0.0.2"}],"reason":""}"#, &shell.locale.catalog, 0);
        sender.taildrop_done(1, 0, r#"{"BackendState":"Running","Self":{"Capabilities":["https://tailscale.com/cap/file-sharing"]},"Peer":{"peer":{"HostName":"Receiver","DNSName":"receiver.invalid.","Online":true,"TaildropTarget":1}}}"#, "", &shell.locale.catalog);
        shell.plugin_fixture_dropbox(1, "Up to date");
        shell.provider_refresh(window, cx);
    });
}

#[gpui::test]
fn plugin_menu_runtime_builtins_use_declared_targets_and_localsend_receives_the_marked_files(
    cx: &mut TestAppContext,
) {
    let root = scratch();
    let (plugin, runtime) = plugin("checksums", &root);
    let mut fixture = open(cx, root.clone(), plugin);
    fixture.menu(cx, 1, vec![0]);
    ready_builtins(&fixture, cx);
    let entries = fixture.entries(cx);
    assert!(entries.iter().any(|r| r.id == "localsend" && !r.disabled));
    assert!(entries
        .iter()
        .any(|r| r.id == "taildrop" && r.action == "plugin-send:flea.tailscale" && !r.disabled));
    assert!(entries.iter().any(|r| r.id == "dropbox" && !r.disabled));
    let validate = fixture.choose(cx, "localsend", "Receiver").unwrap();
    fixture.validated(cx, &validate, &[root.join("notes.txt")]);
    let send = fixture
        .drain()
        .into_iter()
        .find(|r| {
            r.get("c").and_then(Json::as_str) == Some("localsend")
                && r.get("op").and_then(Json::as_str) == Some("send")
        })
        .unwrap();
    assert_eq!(
        send.get("paths"),
        Some(&Json::Arr(vec![Json::Str(
            root.join("notes.txt").to_string_lossy().into()
        )]))
    );
    for (id, row) in [
        ("flea.localsend", "localsend"),
        ("flea.tailscale", "taildrop"),
        ("flea.dropbox", "dropbox"),
    ] {
        fixture.menu(cx, 0, vec![0]);
        ready_builtins(&fixture, cx);
        update(cx, fixture.handle, |shell, window, cx| {
            shell
                .menu
                .plugin_runtime
                .plugins
                .iter_mut()
                .find(|p| p.manifest.id == id)
                .unwrap()
                .enabled = false;
            shell.provider_refresh(window, cx);
        });
        assert!(
            !fixture.entries(cx).iter().any(|r| r.id == row),
            "disabled {id} contributes no row"
        );
        assert!(fixture
            .entries(cx)
            .windows(2)
            .all(|pair| !pair[0].separator || !pair[1].separator));
        update(cx, fixture.handle, |shell, window, cx| {
            let plugin = shell
                .menu
                .plugin_runtime
                .plugins
                .iter_mut()
                .find(|p| p.manifest.id == id)
                .unwrap();
            plugin.enabled = true;
            plugin.failure = Some(manifest::Invalid::new("start", "failed"));
            shell.plugin_provider_states();
            shell.provider_refresh(window, cx);
        });
        assert!(
            !fixture.entries(cx).iter().any(|r| r.id == row),
            "failed {id} contributes no row"
        );
    }
    runtime.shutdown();
}

#[gpui::test]
fn plugin_menu_runtime_builtin_unavailable_rows_have_no_state_word_and_keep_the_red_role(
    cx: &mut TestAppContext,
) {
    let root = scratch();
    let (plugin, runtime) = plugin("checksums", &root);
    let mut fixture = open(cx, root, plugin);
    fixture.menu(cx, 0, vec![0]);
    ready_builtins(&fixture, cx);
    let widths = update(cx, fixture.handle, |shell, _, _| {
        let menu = shell.menu.menu.borrow();
        let card = menu.card.as_ref().unwrap();
        card.entries
            .iter()
            .zip(&card.built.lines)
            .filter(|(row, _)| row.brand.is_some())
            .map(|(row, line)| (row.id.to_string(), line.wanted))
            .collect::<Vec<_>>()
    });
    assert!(!widths.is_empty(), "the menu draws the built-in send rows");
    update(cx, fixture.handle, |shell, window, cx| {
        let sender = &mut shell.menu.providers.sides[0];
        sender.menu_opened(
            &Pipe,
            &Facts {
                tailscale_command: "true".into(),
                send_command: "true".into(),
                localsend_command: "true".into(),
                dropbox_command: "true".into(),
                dropbox_ready: true,
                ..Default::default()
            },
            None,
            &[],
            20_000,
        );
        sender.line(
            r#"{"t":"localsendpeers","id":2,"peers":[],"reason":"offline"}"#,
            &shell.locale.catalog,
            20_000,
        );
        sender.taildrop_done(
            2,
            0,
            r#"{"BackendState":"NeedsLogin"}"#,
            "",
            &shell.locale.catalog,
        );
        shell.plugin_fixture_dropbox(2, "Dropbox isn't running!");
        shell.provider_refresh(window, cx);
    });
    update(cx, fixture.handle, |shell, _, _| {
        let menu = shell.menu.menu.borrow();
        let card = menu.card.as_ref().unwrap();
        let brands: Vec<_> = card
            .entries
            .iter()
            .zip(&card.built.lines)
            .filter(|(row, _)| row.brand.is_some())
            .collect();
        assert_eq!(
            brands.len(),
            widths.len(),
            "every unavailable row stays in the menu"
        );
        for (row, line) in brands {
            assert!(row.disabled, "{} remains unavailable", row.id);
            assert!(line.word.is_none(), "{} has no state word", row.id);
            assert_eq!(line.ink, crate::g2::context_menus::lines::Ink::Errored);
            assert_eq!(
                line.wanted,
                widths.iter().find(|(id, _)| id == &row.id).unwrap().1
            );
        }
        assert!(
            !shell.menu.providers.sides[0].row_on(Role::Tailscale)
                || shell.menu.providers.sides[0]
                    .peers_for(Role::Tailscale)
                    .is_empty()
        );
    });
    runtime.shutdown();
}

#[gpui::test]
fn plugin_menu_runtime_shelf_row_requires_legacy_switch_and_a_healthy_declared_provider(
    cx: &mut TestAppContext,
) {
    let root = scratch();
    let (plugin, runtime) = plugin("checksums", &root);
    let mut fixture = open(cx, root, plugin);
    fixture
        .plugins
        .iter_mut()
        .find(|p| p.manifest.id == "flea.shelf")
        .unwrap()
        .enabled = true;
    update(cx, fixture.handle, |shell, _, _| {
        shell.launch.settings.data = flea::uistate::from_file(r#"{"shelf":{"enabled":true}}"#);
    });
    fixture.menu(cx, 0, vec![0]);
    assert!(fixture.entries(cx).iter().any(|row| row.id == "shelf"));
    update(cx, fixture.handle, |shell, window, cx| {
        shell
            .menu
            .plugin_runtime
            .plugins
            .iter_mut()
            .find(|p| p.manifest.id == "flea.shelf")
            .unwrap()
            .failure = Some(manifest::Invalid::new("start", "failed"));
        shell.provider_refresh(window, cx);
    });
    assert!(!fixture.entries(cx).iter().any(|row| row.id == "shelf"));
    update(cx, fixture.handle, |shell, window, cx| {
        let plugin = shell
            .menu
            .plugin_runtime
            .plugins
            .iter_mut()
            .find(|p| p.manifest.id == "flea.shelf")
            .unwrap();
        plugin.failure = None;
        plugin.manifest.kinds.clear();
        shell.provider_refresh(window, cx);
    });
    assert!(!fixture.entries(cx).iter().any(|row| row.id == "shelf"));
    update(cx, fixture.handle, |shell, window, cx| {
        let plugin = shell
            .menu
            .plugin_runtime
            .plugins
            .iter_mut()
            .find(|p| p.manifest.id == "flea.shelf")
            .unwrap();
        plugin.manifest.kinds = vec!["shelf".into()];
        plugin.enabled = false;
        shell.provider_refresh(window, cx);
    });
    assert!(!fixture.entries(cx).iter().any(|row| row.id == "shelf"));
    update(cx, fixture.handle, |shell, window, cx| {
        shell
            .menu
            .plugin_runtime
            .plugins
            .iter_mut()
            .find(|p| p.manifest.id == "flea.shelf")
            .unwrap()
            .enabled = true;
        shell.launch.settings.data = flea::uistate::from_file(r#"{"shelf":{"enabled":false}}"#);
        shell.provider_refresh(window, cx);
    });
    assert!(!fixture.entries(cx).iter().any(|row| row.id == "shelf"));
    runtime.shutdown();
}
