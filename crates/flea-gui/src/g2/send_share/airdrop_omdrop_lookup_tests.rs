use super::*;

#[test]
fn a_device_the_next_lookup_does_not_report_leaves_the_menu_and_a_new_name_replaces_the_old() {
    let i18n = english();
    let (mut om, acts) = opened(ON);
    let (browse, _) = started(&acts, Proc::Browse).unwrap();
    let old = r#"{"mac": "ae:09:65:f1:05:c3", "rssi": -46, "name": "hume"}"#;
    om.line(Proc::Browse, browse, PHONE);
    om.line(Proc::Browse, browse, old);
    let acts = om.ended(Proc::Browse, browse, 0, 40_000, &i18n);
    let (browse, _) = started(&acts, Proc::Browse).expect("an open menu looks again");
    let renamed = r#"{"mac": "f2:f2:ee:c4:5a:f7", "rssi": null, "name": "Jane's iPad"}"#;
    assert_eq!(om.line(Proc::Browse, browse, renamed), [Act::Refresh]);
    om.line(
        Proc::Browse,
        browse,
        r#"{"mac": "66:95:e5:97:78:8a", "rssi": -44, "name": "hume"}"#,
    );
    let names = |om: &Omdrop| {
        om.devices()
            .into_iter()
            .map(|(p, _)| (p.id, p.label))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&om).len(),
        3,
        "the old address stays until the lookup ends"
    );
    assert!(om
        .ended(Proc::Browse, browse, 0, 80_000, &i18n)
        .contains(&Act::Refresh));
    assert_eq!(
        names(&om),
        [
            ("f2:f2:ee:c4:5a:f7".to_string(), "Jane's iPad".to_string()),
            ("66:95:e5:97:78:8a".to_string(), "hume".to_string()),
        ]
    );
}

#[test]
fn a_failed_lookup_keeps_the_devices_already_listed() {
    let i18n = english();
    let (mut om, acts) = opened(ON);
    let (browse, _) = started(&acts, Proc::Browse).unwrap();
    om.line(Proc::Browse, browse, PHONE);
    let acts = om.ended(Proc::Browse, browse, 0, 40_000, &i18n);
    let (browse, _) = started(&acts, Proc::Browse).unwrap();
    om.ended(Proc::Browse, browse, 4, 80_000, &i18n);
    assert_eq!(om.devices().len(), 1);
}
