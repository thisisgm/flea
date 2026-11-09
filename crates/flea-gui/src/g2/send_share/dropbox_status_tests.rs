// The bounded daemon job owns its generation and readiness even when a menu closes or reopens.
use super::*;
use crate::g2::send_share::seams::Wire;
use flea::ui_logic_send_share::providers::Facts;

struct Pipe;
impl Wire for Pipe {
    fn send(&self, _: String) -> bool {
        true
    }
}

fn open(sender: &mut ProviderSend) -> Vec<Action> {
    sender.menu_opened(
        &Pipe,
        &Facts {
            dropbox_command: "/usr/bin/dropbox-cli".into(),
            dropbox_ready: true,
            ..Default::default()
        },
        None,
        &[],
        0,
    )
}

fn generation(actions: &[Action]) -> u64 {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Run(RunKind::DropboxStatus(generation), _) => Some(*generation),
            _ => None,
        })
        .expect("one captured Dropbox status job")
}

#[test]
fn followup_provider_dropbox_readiness_rejects_stale_and_failed_daemon_answers() {
    let i18n = crate::test_catalogs::english();
    let mut sender = ProviderSend::default();
    let first = generation(&open(&mut sender));
    assert!(!sender.dropbox_ready());
    sender.dropbox_done(first + 1, 0, "Up to date", "", &i18n);
    assert!(sender.db.checking);
    assert!(
        !sender.dropbox_ready(),
        "an unrelated answer cannot make it ready"
    );
    sender.dropbox_done(first, 0, "Dropbox isn't running!", "", &i18n);
    assert!(!sender.dropbox_ready());
    let next = generation(&open(&mut sender));
    assert!(next > first);
    sender.dropbox_done(first, 0, "Up to date", "", &i18n);
    assert!(sender.db.checking);
    sender.dropbox_done(next, 0, "Up to date", "", &i18n);
    assert!(sender.dropbox_ready());
    let timeout = generation(&open(&mut sender));
    sender.dropbox_done(timeout, 137, "", "", &i18n);
    assert!(!sender.dropbox_ready());
    assert_eq!(
        sender.db.status.reason,
        i18n.tr("send.providers.dropbox_timeout")
    );
}

#[test]
fn followup_provider_dropbox_single_flight_refresh_obeys_the_menu_lifetime() {
    let i18n = crate::test_catalogs::english();
    let mut sender = ProviderSend::default();
    let first = generation(&open(&mut sender));
    assert!(
        open(&mut sender).is_empty(),
        "a second open never starts a second process"
    );
    let again = sender.dropbox_done(first, 0, "Up to date", "", &i18n);
    let second = generation(&again);
    assert!(second > first);
    assert!(open(&mut sender).is_empty());
    sender.menu_closed();
    let done = sender.dropbox_done(second, 0, "Up to date", "", &i18n);
    assert!(
        done.is_empty(),
        "a closed menu never rearms a waiting status check"
    );
    assert!(
        sender.dropbox_ready(),
        "the completed result remains available"
    );
}
