use super::*;

#[test]
fn a_radio_that_goes_down_before_its_ten_minutes_end_says_why() {
    let i18n = english();
    let (mut air, _) = on_with_menu();
    let radio = air.gens[slot(Proc::Radio)];
    air.line(
        Proc::Radio,
        radio,
        "owly: the radio went down: link lost",
        0,
        &english(),
    );
    let acts = air.ended(Proc::Radio, radio, 1, 0, &i18n);
    assert_eq!(
        acts[0],
        Act::Say("AirDrop · the radio went down: link lost".into(), true)
    );
    assert_eq!(air.radio, Radio::Off);
}

#[test]
fn a_receiver_that_exits_takes_the_radio_down_and_only_a_failure_says_why() {
    let i18n = english();
    let (mut air, _) = on_with_menu();
    let receive = air.gens[slot(Proc::Receive)];
    let acts = air.ended(Proc::Receive, receive, owly::TIMEOUT_EXPIRED, 0, &i18n);
    assert_eq!(
        acts,
        [
            Act::Stop(Proc::Radio),
            Act::Stop(Proc::Browse),
            Act::Refresh
        ],
        "its ten minutes ran out"
    );
    assert_eq!(air.radio, Radio::Off);
    let (mut air, _) = on_with_menu();
    let receive = air.gens[slot(Proc::Receive)];
    air.line(
        Proc::Receive,
        receive,
        "owly: awdl0: no such device",
        0,
        &english(),
    );
    let acts = air.ended(Proc::Receive, receive, 2, 0, &i18n);
    assert_eq!(
        acts[0],
        Act::Say("AirDrop · awdl0: no such device".into(), true)
    );
    assert!(acts.contains(&Act::Stop(Proc::Radio)) && !acts.contains(&Act::Stop(Proc::Receive)));
    assert!(!air.page("/d", "h").receiving);
}

#[test]
fn a_failed_browse_is_not_started_again_until_the_menu_opens_again() {
    let (mut air, browse) = on_with_menu();
    assert!(air.ended(Proc::Browse, browse, 2, 0, &english()).is_empty());
    air.menu_closed();
    assert!(started(
        &air.menu_opened(BROWSE_GAP_MS, &Seen::default()),
        Proc::Browse
    )
    .is_some());
}

#[test]
fn a_doctor_still_running_when_owly_goes_away_never_lands() {
    let mut air = AirDrop::default();
    air.facts("/usr/bin/owly");
    let acts: [Act; 1] = air.ask_doctor(0).try_into().unwrap();
    let [Act::Doctor(generation, _)] = acts else {
        panic!()
    };
    air.facts("");
    assert!(air.doctor_done(generation, 0, READY, 1).is_empty());
    assert_eq!(air.adapter.readiness, Readiness::Missing);
}
