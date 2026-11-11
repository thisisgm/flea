//! tests/unlock-host.qml and tests/unlock-poller.qml against the host: the no-rail open, the rail follow, and the one-shot name poller.
use super::devices::LIST_TIMEOUT_MS;
use super::host::{Effect, Host};
use super::process::Outcome;
use super::seams::RailView;
use super::unlock::Effect as CardEffect;
use flea::ui_logic_device_rail::unlock::unlocked_note;
use flea::ui_logic_device_rail::{Group, Kind, RailEntry};

const ROW: &str = r#"{"blockdevices":[{"name":"cap-loop9","path":"/dev/loop9","label":null,"mountpoints":[null],"rm":true,"tran":null,"size":33554432,"type":"loop","model":null,"fstype":"crypto_LUKS","fsver":"2"}]}"#;
const EMPTY: &str = r#"{"blockdevices":[]}"#;

fn listed(body: &str) -> Outcome {
    Outcome {
        code: 0,
        stdout: body.into(),
        stderr: String::new(),
    }
}

fn locked(device: &str, label: &str) -> RailEntry {
    RailEntry {
        group: Group::Device,
        kind: Kind::Volume,
        device: device.into(),
        label: label.into(),
        locked: true,
        size: Some(1),
        fstype: "crypto_LUKS".into(),
        fsver: "2".into(),
        ..RailEntry::default()
    }
}

fn rail() -> RailView {
    RailView {
        places: 2,
        network: 1,
        devices: vec![locked("/dev/sdb1", "Backup")],
        cursor: None,
    }
}

fn label(host: &Host) -> String {
    host.card
        .as_ref()
        .map(|c| c.label.clone())
        .unwrap_or_default()
}

fn unlocked(host: &mut Host, rail: Option<&RailView>) -> Vec<Effect> {
    let effect = CardEffect::Unlocked {
        path: "/run/media/gm/Backup".into(),
        label: "Backup".into(),
        note: unlocked_note("Backup"),
    };
    host.card_effects(vec![effect], rail)
}

fn lists(effects: &[Effect]) -> usize {
    effects
        .iter()
        .filter(|e| matches!(e, Effect::NameList(_)))
        .count()
}

#[test]
fn no_rail_unlock_opens_the_folder_and_says_the_note() {
    let mut host = Host::default();
    host.open_for("/dev/sdb1", "Backup", 500_000_000_000, "crypto_LUKS", None);
    assert!(host.opened(), "no rail card opens");
    let effects = unlocked(&mut host, None);
    assert!(
        matches!(&effects[..], [Effect::OpenInPane(p), Effect::Say(n, false)] if p == "/run/media/gm/Backup" && *n == unlocked_note("Backup"))
    );
    assert_eq!(host.pending_path, "", "no rail unlock stashes no follow");
}

#[test]
fn rail_unlock_follows_the_row_once_it_lists() {
    let mut host = Host::default();
    let mut view = rail();
    view.cursor = Some(3);
    host.open_for("/dev/sdb1", "Backup", 0, "crypto_LUKS", Some(&view));
    let effects = unlocked(&mut host, Some(&view));
    assert!(matches!(
        &effects[..],
        [
            Effect::RailOpened(_),
            Effect::PollDevices,
            Effect::Say(_, false)
        ]
    ));
    assert_eq!(host.pending_path, "/run/media/gm/Backup");
    let mut mounted = RailEntry {
        locked: false,
        mounted: true,
        path: "/run/media/gm/Backup".into(),
        ..locked("/dev/dm-0", "Backup")
    };
    mounted.device = "/dev/dm-0".into();
    view.devices = vec![mounted];
    let effects = host.rail_rebuilt(Some(&view));
    assert!(matches!(&effects[..], [Effect::SetCursor(3)]));
    assert_eq!(host.pending_path, "");
}

#[test]
fn moving_the_cursor_ends_the_follow() {
    let mut host = Host::default();
    let mut view = rail();
    view.cursor = Some(3);
    host.open_for("/dev/sdb1", "Backup", 0, "crypto_LUKS", Some(&view));
    unlocked(&mut host, Some(&view));
    view.cursor = Some(0);
    assert!(host.rail_rebuilt(Some(&view)).is_empty());
    assert_eq!(host.pending_path, "");
}

#[test]
fn remembered_rail_reads_the_luks_version_and_keeps_the_passed_label() {
    let mut host = Host::default();
    host.open_for("/dev/sdb1", "leaf", 0, "crypto_LUKS", Some(&rail()));
    let card = host.card.as_ref().unwrap();
    assert_eq!(card.crypto, "LUKS2");
    assert_eq!(card.label, "leaf");
    assert!(!host.name_polling(), "no pending name runs no poller");
    assert!(host.name_tick(0).is_empty());
}

#[test]
fn first_listing_names_the_card_and_ends_the_poller() {
    let mut host = Host::default();
    host.open_for_device("/dev/loop9", None);
    assert_eq!(label(&host), "loop9", "pending card shows the leaf");
    assert_eq!(host.unnamed_device, "/dev/loop9");
    assert!(host.name_polling());
    let effects = host.name_tick(0);
    assert_eq!(lists(&effects), 1);
    assert!(matches!(&effects[0], Effect::NameList(job) if job.argv[0] == "lsblk"));
    assert!(host.name_tick(100).is_empty(), "one listing at a time");
    assert!(host.name_exited(&listed(ROW), 200, None).is_empty());
    assert_eq!(label(&host), "cap-loop9", "first listing names the card");
    assert_eq!(host.card.as_ref().unwrap().crypto, "LUKS2");
    assert_eq!(host.unnamed_device, "");
    assert!(
        !host.name_polling() && host.name_listings().is_none(),
        "first listing stops the poller"
    );
}

#[test]
fn rowless_listing_keeps_the_leaf_and_ends_the_wait() {
    let mut host = Host::default();
    host.open_for_device("/dev/ghost", None);
    host.name_tick(0);
    host.name_exited(&listed(EMPTY), 10, None);
    assert_eq!(label(&host), "ghost");
    assert_eq!(host.unnamed_device, "");
    assert!(!host.name_polling());
}

#[test]
fn touched_card_stops_the_poller_and_a_late_listing_names_nothing() {
    let mut host = Host::default();
    host.open_for_device("/dev/loop9", None);
    assert_eq!(lists(&host.name_tick(0)), 1);
    host.card.as_mut().unwrap().type_text("typed");
    let effects = host.name_tick(10);
    assert!(
        matches!(&effects[..], [Effect::NameKill]),
        "a touch drops the running lsblk"
    );
    assert!(
        host.name_listings().is_none(),
        "touched card builds once then stops"
    );
    assert!(host.name_exited(&listed(ROW), 20, None).is_empty());
    assert_eq!(
        label(&host),
        "loop9",
        "late listing keeps the touched label"
    );
    host.name_device(None);
    assert_eq!(
        host.unnamed_device, "",
        "late listing ends the touched wait"
    );
}

#[test]
fn erase_after_a_touch_keeps_the_latch() {
    let mut host = Host::default();
    host.open_for_device("/dev/loop9", None);
    host.name_tick(0);
    host.card.as_mut().unwrap().type_text("x");
    host.name_tick(10);
    host.card.as_mut().unwrap().backspace();
    assert!(
        !host.card.as_ref().unwrap().touched(),
        "the field is empty again"
    );
    assert!(!host.name_polling(), "erase keeps the latch");
    assert!(
        host.name_tick(20).is_empty(),
        "erased card starts no second lsblk"
    );
    assert_eq!(label(&host), "loop9");
}

#[test]
fn timed_out_listing_lists_again_at_the_bound() {
    let mut host = Host::default();
    host.open_for_device("/dev/loop9", None);
    host.name_tick(0);
    assert!(
        host.name_deadline().is_some_and(|at| at <= LIST_TIMEOUT_MS),
        "the bound is armed"
    );
    let effects = host.name_tick(LIST_TIMEOUT_MS);
    assert!(
        effects.iter().any(|e| matches!(e, Effect::NameBound)),
        "the bound kills without dropping the poller"
    );
    let effects = host.name_exited(&Outcome::code(-1), LIST_TIMEOUT_MS + 5, None);
    assert_eq!(lists(&effects), 1, "timed out poller polls again");
    assert_eq!(host.name_listings(), Some(2));
    assert_eq!(label(&host), "loop9", "timed out card keeps the leaf");
    assert_eq!(
        host.unnamed_device, "/dev/loop9",
        "timed out card still waits"
    );
    host.name_exited(&listed(ROW), LIST_TIMEOUT_MS + 50, None);
    assert_eq!(label(&host), "cap-loop9", "retried listing names the card");
}
