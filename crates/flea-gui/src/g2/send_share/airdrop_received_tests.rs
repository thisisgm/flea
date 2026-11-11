//! What the receiver saved after Accept, said in the footer: split from airdrop_machine_tests.rs at its cap.
use super::*;

// The receiver is up: its generation, at time zero.
fn receiving() -> (AirDrop, u64) {
    let i18n = english();
    let mut air = window(None);
    let radio = start(&on(&mut air, &i18n), Proc::Radio).unwrap();
    let acts = air.line(Proc::Radio, radio, UP, 0, &i18n);
    (air, start(&acts, Proc::Receive).unwrap())
}

#[test]
fn files_the_receiver_saved_are_said_by_name_then_counted_within_one_transfer() {
    let i18n = english();
    let (mut air, receive) = receiving();
    let mut say = |text: &str, at| air.line(Proc::Receive, receive, text, at, &i18n);
    assert_eq!(
        say("/d/IMG_0001.HEIC", 10),
        [Act::Say(
            "Received IMG_0001.HEIC with AirDrop".into(),
            false
        )]
    );
    assert_eq!(
        say("/d/IMG_0002.HEIC", 20),
        [Act::Say("Received 2 items with AirDrop".into(), false)]
    );
    assert_eq!(
        say("/d/notes.txt", 20 + RECEIVED_GROUP_MS),
        [Act::Say("Received notes.txt with AirDrop".into(), false)],
        "a later transfer starts its own count"
    );
    assert_eq!(
        say(r#""https://example.com/""#, 30_000),
        [Act::Say(
            "Received https://example.com/ with AirDrop".into(),
            false
        )],
        "a link is shown, never opened"
    );
    assert_eq!(
        say("transfer failed: connection reset", 31_000),
        [Act::Say("AirDrop · connection reset".into(), true)]
    );
    assert_eq!(
        say("the request was withdrawn", 32_000),
        [Act::Say(
            "AirDrop could not finish the transfer.".into(),
            true
        )]
    );
    assert!(say("seen by fe80::1%awdl0", 33_000).is_empty());
    assert!(
        air.line(Proc::Receive, receive - 1, "/d/old.txt", 34_000, &i18n)
            .is_empty(),
        "a replaced receiver says nothing"
    );
}
