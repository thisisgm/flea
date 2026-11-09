use super::*;
use crate::i18n::Paths;
use std::path::PathBuf;

fn english() -> I18n {
    let none = PathBuf::from("/nonexistent/flea-airdrop");
    I18n::load(Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}

// minipc's own report, 2026-11-07, from owly 0.4.0 built in a sandbox without owlyd installed.
const MINIPC: &str = r#"{"card":{"phy":"phy0"},"channel":6,"device":"8086:24fd","driver":"iwlwifi","hardware":true,
"issues":["Intel cards start no ad-hoc cell on 5 GHz, so they run on channel 6."],
"missing":[{"id":"backend","say":"Owly's radio service, owlyd, can't be reached"}],"mode":"exclusive","station":"wlp2s0","status":"tested"}"#;

#[test]
fn a_card_without_owlyd_needs_the_service_and_keeps_its_name() {
    let adapter = doctor(MINIPC, 1);
    assert_eq!(adapter.readiness, Readiness::NoService);
    assert_eq!(adapter.card, "Intel 8265");
    assert!(!adapter.ready());
    assert_eq!(
        adapter_value(&adapter, &english()),
        "Intel 8265, needs owlyd"
    );
}

#[test]
fn a_ready_report_works_and_an_unknown_device_names_its_driver() {
    let ready = r#"{"device":"8086:2725","driver":"iwlwifi","hardware":true,"status":"tested","missing":[]}"#;
    let adapter = doctor(ready, 0);
    assert!(adapter.ready());
    assert_eq!(
        adapter_value(&adapter, &english()),
        "Intel AX210, works with AirDrop"
    );
    let unknown = r#"{"device":"1234:5678","driver":"mt7915e","hardware":true,"status":"tested","missing":[]}"#;
    assert_eq!(doctor(unknown, 0).card, "mt7915e");
    // A report with nothing missing but a failing exit is not trusted as ready.
    assert_eq!(doctor(ready, 1).readiness, Readiness::NotReady);
}

#[test]
fn broken_untested_cardless_and_unreadable_reports_each_say_why() {
    let broken = r#"{"device":"8086:24fd","driver":"iwlwifi","hardware":true,"status":"broken","missing":[]}"#;
    assert_eq!(doctor(broken, 0).readiness, Readiness::Broken);
    let untested = r#"{"device":"aaaa:bbbb","driver":"x","hardware":true,"status":"untested","missing":[{"id":"radio","say":"allow_untested"}]}"#;
    assert_eq!(doctor(untested, 1).readiness, Readiness::Untested);
    assert_eq!(
        doctor(
            r#"{"hardware":false,"missing":[{"id":"hardware","say":"no card"}]}"#,
            1
        )
        .readiness,
        Readiness::NoCard
    );
    let channel = r#"{"device":"14c3:0717","hardware":true,"status":"tested","missing":[{"id":"radio","say":"channel 36"}]}"#;
    assert_eq!(doctor(channel, 1).readiness, Readiness::NotReady);
    assert_eq!(doctor("", 137).readiness, Readiness::NotReady);
    assert_eq!(doctor("not json", 2).readiness, Readiness::NotReady);
    let i18n = english();
    assert_eq!(
        adapter_value(&doctor(broken, 0), &i18n),
        "Intel 8265, cannot do AirDrop"
    );
    assert_eq!(
        adapter_value(
            &Adapter {
                readiness: Readiness::Missing,
                card: String::new()
            },
            &i18n
        ),
        "Needs the owly package"
    );
}

#[test]
fn browse_lines_read_rust_quoted_names_and_pick_the_device_mark() {
    let peer =
        browse_line(r#"3a9f0c2be41d   "Jane's iPhone" ([fe80::1c2:3ff:fe44:5566%awdl0]:8770)"#)
            .expect("a receiver");
    assert_eq!(
        peer,
        Peer {
            id: "3a9f0c2be41d".into(),
            name: "Jane's iPhone".into(),
            phone: true
        }
    );
    let mac =
        browse_line(r#"0f1e2d3c4b5a   "MacBook Air" ([fe80::1%7]:8770)"#).expect("a receiver");
    assert!(!mac.phone);
    let quoted =
        browse_line(r#"aa   "Ben \"B\" iPad\u{2019}s \\ x" ([fe80::2%7]:8770)"#).expect("escapes");
    assert_eq!(quoted.name, "Ben \"B\" iPad\u{2019}s \\ x");
    assert!(quoted.phone);
    assert!(browse_line("not advertising over Bluetooth: no adapter").is_none());
    assert!(browse_line(r#"aa "" ([fe80::2]:8770)"#).is_none());
    assert!(browse_line(r#"   "No ID" ([fe80::2]:8770)"#).is_none());
    assert!(browse_line(r#"aa "unterminated"#).is_none());
}

#[test]
fn send_verdicts_follow_owlys_stdout_and_exit_code() {
    assert_eq!(verdict("delivered\n", "", 0), Verdict::Delivered);
    assert_eq!(verdict("declined\n", "", 1), Verdict::Declined);
    assert_eq!(
        verdict("failed: the receiver went away\n", "", 2),
        Verdict::Failed("the receiver went away".into())
    );
    assert_eq!(
        verdict(
            "",
            "looking for \"x\" for 30 s...\nowly: no such receiver found\n",
            2
        ),
        Verdict::Failed("no such receiver found".into())
    );
    // A killed or crashed run is a failure with no words of its own.
    assert_eq!(verdict("", "", 137), Verdict::Failed(String::new()));
    assert_eq!(
        verdict("delivered\n", "", 2),
        Verdict::Failed(String::new())
    );
}

#[test]
fn progress_steps_and_the_radio_line_parse() {
    assert_eq!(
        progress("\r1048576 / 2400000 bytes"),
        Some((1048576, 2400000))
    );
    assert_eq!(progress("accepted"), None);
    assert_eq!(progress("1 / x bytes"), None);
    assert!(radio_is_up(
        "awdl0 is up: fe80::1%awdl0 (02:00:00:00:00:01). Ctrl-C takes it down."
    ));
    assert!(!radio_is_up("bringing the radio up..."));
    assert_eq!(
        reason("bringing the radio up...\nowly: owlyd can't be reached (x)\n"),
        "owlyd can't be reached (x)"
    );
    assert_eq!(reason(""), "");
}

#[test]
fn every_run_is_bounded_and_files_follow_the_double_dash() {
    assert_eq!(
        doctor_argv("/usr/bin/owly"),
        [
            "timeout",
            "--signal=KILL",
            "10",
            "/usr/bin/owly",
            "doctor",
            "--json"
        ]
    );
    assert_eq!(radio_argv("owly"), ["owly", "radio", "up", "--for", "600"]);
    assert_eq!(
        receive_argv("owly", "/home/u/Downloads"),
        [
            "timeout",
            "600",
            "owly",
            "receive",
            "--dir",
            "/home/u/Downloads"
        ]
    );
    assert_eq!(browse_argv("owly"), ["owly", "browse", "--seconds", "30"]);
    let files = vec!["/a/-x.png".to_string(), "/a/b c.txt".to_string()];
    assert_eq!(send_argv("owly", "-Jane", &[])[2], "--to=-Jane");
    assert_eq!(
        send_argv("owly", "Jane's iPhone", &files),
        [
            "owly",
            "send",
            "--to=Jane's iPhone",
            "--",
            "/a/-x.png",
            "/a/b c.txt"
        ]
    );
}

#[test]
fn the_send_request_and_its_answer_round_trip_and_each_answer_has_its_sentence() {
    let line = send_line(3, "3a9f", "Jane's \"iPhone\"", &["/a/b c.png".to_string()]);
    assert_eq!(
        line,
        r#"{"c":"airdrop","op":"send","id":3,"peer":"3a9f","name":"Jane's \"iPhone\"","paths":["/a/b c.png"]}"#
    );
    let answer = r#"{"t":"airdropsent","id":3,"ok":false,"code":"failed","reason":"timed out"}"#;
    assert_eq!(
        sent(answer),
        Some((3, "failed".to_string(), "timed out".to_string()))
    );
    assert_eq!(sent(r#"{"t":"localsendsent","id":3,"ok":true}"#), None);
    assert_eq!(sent("garbage"), None);
    let i18n = english();
    assert_eq!(
        sent_said(&i18n, "Jane's iPhone", "delivered", "").as_deref(),
        Some("AirDrop finished the transfer.")
    );
    assert_eq!(
        sent_said(&i18n, "Jane's iPhone", "declined", "").as_deref(),
        Some("Jane's iPhone declined the files.")
    );
    assert_eq!(
        sent_said(&i18n, "x", "failed", "timed out").as_deref(),
        Some("AirDrop · timed out")
    );
    assert_eq!(
        sent_said(&i18n, "x", "failed", "").as_deref(),
        Some("AirDrop could not finish the transfer.")
    );
    assert_eq!(
        sent_said(&i18n, "x", "missing", "").as_deref(),
        Some("AirDrop · Needs the owly package")
    );
    assert_eq!(
        sent_said(&i18n, "x", "stale", "changed").as_deref(),
        Some("That menu is out of date. Reopen it.")
    );
    assert_eq!(sent_said(&i18n, "x", "cancelled", ""), None);
    assert_eq!(
        sending(&i18n, "Mac", &["/a/b.png".to_string()]),
        "Sending b.png to Mac with AirDrop"
    );
    assert_eq!(
        radio_failed(&i18n, "owly: the radio is busy\n"),
        "AirDrop · the radio is busy"
    );
    assert_eq!(radio_failed(&i18n, ""), "AirDrop could not turn on.");
}
