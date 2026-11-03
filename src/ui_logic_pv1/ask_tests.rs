// previewask.js and extthumbs.js value checks, each labelled as in the JS; Menu.js, Settings.js and QML-source checks stay out.
use super::ask::*;
use super::property::{thumbs_label, thumbs_status};
use super::thumbs::{Entry, Map};

fn ids(v: &[usize]) -> String {
    v.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")
}

// The JS base object: readable, storage known, one selection, cursor row 3, thumbnailable, unasked, local.
fn base(switches: &ClassSwitches) -> Ask<'_> {
    Ask {
        can_read: true,
        storage_known: true,
        selection_count: 1,
        index: Some(3),
        thumbnailable: true,
        asked: false,
        storage_class: "local",
        switches,
    }
}

// The JS earlyBase clock: settled a long time after the last move, key on row 3, last move on row 2.
fn idle_clock(now: i64) -> Clock<'static> {
    Clock { now, last_move_at: 0, interval: 120, key: "/d\n3", last_move_key: "/d\n2", armed: false }
}

#[test]
fn cursor_ask_is_one_gate_shared_by_early_and_load() {
    let sw = ClassSwitches::default();
    let work = work_for(&base(&sw)).expect("an idle press asks");
    // an idle press asks the cursor row
    assert_eq!(ids(&work.ask), "3");
    // and carries no drop
    assert_eq!(work.drop.len(), 0);
    // local decodes in full
    assert!(!work.cache_only);
    let net = ClassSwitches { network: Some(true), ..Default::default() };
    // a switched-on class decodes in full
    assert!(!work_for(&Ask { storage_class: "network", ..base(&net) }).expect("asks").cache_only);
    // an idle press passes the settle's own rule
    assert_eq!(ids(&early_work_for(&base(&sw), &idle_clock(1000)).expect("asks").ask), "3");
    // a walk step asks nothing early
    assert!(early_work_for(&base(&sw), &Clock { now: 50, ..idle_clock(1000) }).is_none());
    // a pending repeat asks nothing early
    assert!(early_work_for(&base(&sw), &Clock { now: 1000, key: "/d\n2", last_move_key: "/d\n2", armed: true, ..idle_clock(1000) }).is_none());
    // unknown storage asks nothing early, a NAS image would thumbnail in full
    assert!(early_work_for(&Ask { storage_known: false, ..base(&sw) }, &idle_clock(1000)).is_none());
    // and load's own ask stays held too
    assert!(work_for(&Ask { storage_known: false, ..base(&sw) }).is_none());
    let plain = ClassSwitches::default();
    // a manual-hold class asks nothing early
    assert!(early_work_for(&Ask { storage_class: "network", ..base(&plain) }, &idle_clock(1000)).is_none());
    // load's own ask on an off class asks cache only, the hold is not its gate
    assert_eq!(work_for(&Ask { storage_class: "network", ..base(&plain) }), Some(Work { ask: vec![3], drop: Vec::new(), cache_only: true }));
    // an already asked row asks nothing again
    assert!(early_work_for(&Ask { asked: true, ..base(&sw) }, &idle_clock(1000)).is_none());
    // a multi-selection asks nothing
    assert!(early_work_for(&Ask { selection_count: 3, ..base(&sw) }, &idle_clock(1000)).is_none());
    // an unreadable frame asks nothing
    assert!(early_work_for(&Ask { can_read: false, ..base(&sw) }, &idle_clock(1000)).is_none());
    // a row with no thumbnail asks nothing
    assert!(early_work_for(&Ask { thumbnailable: false, ..base(&sw) }, &idle_clock(1000)).is_none());
    // no row asks nothing
    assert!(early_work_for(&Ask { index: None, ..base(&sw) }, &idle_clock(1000)).is_none());
}

#[test]
fn only_three_classes_name_a_switch() {
    // only the three classes name a switch
    assert_eq!(["network", "phone", "usb", "", "smb"].map(|c| class_of(c).is_some()), [true, true, true, false, false]);
    // and only they hold the menu row
    assert_eq!(["network", "phone", "usb", "", "smb"].map(present), [true, true, true, false, false]);
}

#[test]
fn network_and_phone_ship_off_usb_on() {
    let d = ClassSwitches::default();
    // network and phone ship off
    assert_eq!([class_on("network", &d), class_on("phone", &d)], [false, false]);
    // USB ships on
    assert!(class_on("usb", &d));
    let on = ClassSwitches { network: Some(true), usb: Some(false), ..Default::default() };
    // a stored switch reads back
    assert_eq!([class_on("network", &on), class_on("usb", &on)], [true, false]);
    // local is always on, whatever the file holds
    assert!(class_on("", &on));
}

#[test]
fn off_class_is_cache_only() {
    let d = ClassSwitches::default();
    // an off class is cache-only
    assert_eq!([cache_only("network", &d), cache_only("phone", &d), cache_only("usb", &d)], [true, true, false]);
    let net_on = ClassSwitches { network: Some(true), ..Default::default() };
    // a switched-on class decodes
    assert!(!cache_only("network", &net_on));
    // local never is
    assert!(!cache_only("", &d));
    let global_off = ClassSwitches { thumbnails_off: true, ..Default::default() };
    // and a global Off asks for nothing to gate
    assert!(!cache_only("network", &global_off));
    // the hold is the same condition as the gate
    assert_eq!(manual_hold("phone", &d), cache_only("phone", &d));
}

#[test]
fn menu_labels_name_the_class_and_its_direction() {
    let i18n = super::english();
    let on = ClassSwitches { network: Some(true), phone: Some(true), usb: Some(true), ..Default::default() };
    let off = ClassSwitches { usb: Some(false), ..Default::default() };
    let labels = |s: &ClassSwitches| ["network", "phone", "usb"].map(|c| thumbs_label(c, s, &i18n).unwrap_or_default()).join("|");
    // the row names its class going off
    assert_eq!(labels(&off), "Show network thumbnails|Show phone thumbnails|Show USB thumbnails");
    // and coming on
    assert_eq!(labels(&on), "Hide network thumbnails|Hide phone thumbnails|Hide USB thumbnails");
}

#[test]
fn status_line_says_what_the_toggle_did() {
    let i18n = super::english();
    // the toggle says what it did (network on)
    assert_eq!(thumbs_status("network", true, &i18n).as_deref(), Some("Network thumbnails are on"));
    // (usb off)
    assert_eq!(thumbs_status("usb", false, &i18n).as_deref(), Some("USB thumbnails are off"));
    // (phone on)
    assert_eq!(thumbs_status("phone", true, &i18n).as_deref(), Some("Phone thumbnails are on"));
}

#[test]
fn forget_misses_keeps_refusals_and_hits() {
    let mut st = map_of(&[(3, Entry::File(String::new())), (4, Entry::File("/c/x.png".into())), (5, Entry::CacheMissed), (6, Entry::CacheAsked)]);
    st.forget_misses();
    // a real refusal and a hit stay while both cache-only marks leave
    assert_eq!(st.order.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","), "3,4");
    // (refusal kept as an empty file)
    assert_eq!(st.file.get(&3), Some(&Entry::File(String::new())));
    // (hit kept)
    assert_eq!(st.file_for(4), "/c/x.png");
    // (cache miss removed)
    assert_eq!(st.file.get(&5), None);
    // (cache asked removed)
    assert_eq!(st.file.get(&6), None);
}

#[test]
fn verdict_names_the_cache_gate_and_mode() {
    let d = ClassSwitches::default();
    let net_on = ClassSwitches { network: Some(true), ..Default::default() };
    // the verdict names the current class's cache gate and mode (JS mode undefined, passed as "")
    assert_eq!(verdict("network", &d, ""), (true, "media".to_string()));
    assert_eq!(verdict("network", &net_on, ""), (false, "media".to_string()));
    assert_eq!(verdict("", &d, ""), (false, "media".to_string()));
    // zoom and the column are not inputs of verdict, so only the class switches can move it
    let a = verdict("network", &d, "");
    let usb_off = ClassSwitches { usb: Some(false), ..Default::default() };
    // and other classes move nothing (thumbUsb false)
    assert_eq!(verdict("network", &usb_off, ""), a);
}

#[test]
fn text_limit_per_class() {
    // text on network and phone reads at most 256 KiB
    assert_eq!([text_limit("network"), text_limit("phone")], [262_144, 262_144]);
    // local and USB keep the megabyte
    assert_eq!([text_limit(""), text_limit("usb")], [1_048_576, 1_048_576]);
}

fn map_of(entries: &[(usize, Entry)]) -> Map {
    let mut m = Map::default();
    for (k, e) in entries {
        m.file.insert(*k, e.clone());
        m.order.push(*k);
    }
    m
}
