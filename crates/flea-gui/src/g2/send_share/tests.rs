//! ProviderSend against a fake backend; review round 1 findings with a unit shape have a named case.
use super::seams::{Action, ProviderState, RunKind, Wire};
use super::ProviderSend;
use flea::i18n::{I18n, Paths};
use flea::ui_logic_send_share::providers::{
    Facts, Role, PLUGIN_DROPBOX, PLUGIN_LOCALSEND, PLUGIN_TAILSCALE,
};
use std::cell::RefCell;
use std::path::PathBuf;
mod browser;
mod fixmenus;
mod link;
mod localsend;
mod plugins;
mod taildrop;

#[derive(Default)]
struct Fake {
    lines: RefCell<Vec<String>>,
    dead: RefCell<bool>,
}

impl Wire for Fake {
    fn send(&self, line: String) -> bool {
        if *self.dead.borrow() {
            return false;
        }
        self.lines.borrow_mut().push(line);
        true
    }
}

fn english() -> I18n {
    let none = PathBuf::from("/nonexistent/flea-sd1");
    I18n::load(Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}

fn facts() -> Facts {
    Facts {
        tailscale_command: "/usr/bin/tailscale".into(),
        send_command: "/usr/bin/omarchy-tailscale-send".into(),
        localsend_command: "/usr/bin/localsend-cli".into(),
        localsend_reason: String::new(),
        dropbox_command: "/usr/bin/dropbox-cli".into(),
        dropbox_ready: true,
        ..Facts::default()
    }
}

fn on(plugin: &str) -> ProviderState {
    ProviderState {
        plugin: plugin.into(),
        enabled: true,
        failed: false,
    }
}

fn states() -> Vec<ProviderState> {
    vec![
        on(PLUGIN_TAILSCALE),
        on(PLUGIN_LOCALSEND),
        on(PLUGIN_DROPBOX),
    ]
}

// The tests/js/taildrop.js ready shape, trimmed to the fields status() reads.
fn ready_status() -> String {
    r#"{"BackendState":"Running","Self":{"UserID":1000000000000001,
"Capabilities":["https://tailscale.com/cap/file-sharing"]},
"Peer":{"nodekey:owned":{"HostName":"laptop","DNSName":"laptop.ts.net.","Online":true,
"TaildropTarget":1,"UserID":1000000000000001}}}"#
        .into()
}

// One peer that carries only a tailnet address, so its own name chain ends on the reserved word.
fn address_only_status() -> String {
    r#"{"BackendState":"Running","Self":{"UserID":1000000000000001,
"Capabilities":["https://tailscale.com/cap/file-sharing"]},
"Peer":{"nodekey:ip":{"TailscaleIPs":["100.1.2.3"],"Online":true,
"TaildropTarget":1,"UserID":1000000000000001}}}"#
        .into()
}

// The first status ask of a fresh ProviderSend takes generation 1.
const FIRST_RUN: u64 = 1;

fn status_runs(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|action| matches!(action, Action::Run(RunKind::TaildropStatus(_), _)))
        .count()
}

#[test]
fn uish_misc_unusable_provider_facts_keep_the_disabled_row_and_no_command() {
    let facts = flea::ui_logic_send_share::providers::parse_facts(r#"{"taildrop":{"installed":true,"command":"","reason":"not executable"},"dropbox":{"installed":true,"command":"","reason":"not executable"},"dropboxInfo":"{}"}"#).unwrap();
    assert!(super::installed(Role::Tailscale, &facts));
    assert!(super::installed(Role::Dropbox, &facts));
    assert!(!super::installed(Role::LocalSend, &facts));
    assert!(facts.tailscale_command.is_empty() && facts.dropbox_command.is_empty());
    struct NoWire;
    impl Wire for NoWire {
        fn send(&self, _: String) -> bool {
            panic!("an unusable provider must start no discovery")
        }
    }
    let mut sender = ProviderSend::default();
    assert!(sender.menu_opened(&NoWire, &facts, None, &[], 0).is_empty());
    assert!(sender.row_on(Role::Tailscale) && sender.row_on(Role::Dropbox));
    assert!(sender.dropbox_refresh().is_none());
}
