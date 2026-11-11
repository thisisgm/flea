use super::*;
use crate::i18n::Paths;
use std::path::PathBuf;

fn english() -> I18n {
    let none = PathBuf::from("/nonexistent/flea-airdrop-ask");
    I18n::load(Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}

// owly 0.4.0's receive stderr for two requests, each prompt left open with no newline as owly leaves it.
const TWO_REQUESTS: &str = concat!(
    "receiving as \"minipc\"\n",
    "seen by fe80::1%awdl0\n",
    "Some(\"Jane's iPhone\") (Some(\"iPhone15,2\")) wants to send:\n",
    "  \"IMG_0001.HEIC\" public.heic 2400000 bytes\n",
    "accept? [y/N] ",
    "None (None) wants to send:\n",
    "  \"a.txt\" public.plain-text 3 bytes\n",
    "  \"b <i>.txt\" public.plain-text 4 bytes\n",
    "accept? [y/N] ",
    "\ntimed out\n",
);

#[test]
fn every_prompt_is_answered_from_its_own_request_and_nothing_is_accepted_unasked() {
    let mut asked = Vec::new();
    let mut replies = Vec::new();
    serve(
        TWO_REQUESTS.as_bytes(),
        &mut replies,
        |request| {
            asked.push(request.clone());
            asked.len() == 1
        },
        |_| {},
    );
    assert_eq!(
        asked,
        [
            Request {
                sender: "Jane's iPhone".into(),
                files: vec!["IMG_0001.HEIC".into()],
            },
            Request {
                sender: String::new(),
                files: vec!["a.txt".into(), "b <i>.txt".into()],
            },
        ]
    );
    assert_eq!(String::from_utf8(replies).unwrap(), "y\nn\n");
}

#[test]
fn a_sender_or_file_named_like_the_prompt_is_not_answered_and_answers_stay_one_per_request() {
    let stream = concat!(
        "Some(\"x accept? [y/N] \") (None) wants to send:\n",
        "  \"accept? [y/N] \" public.data 1 bytes\n",
        "  \"b.txt\" public.plain-text 1 bytes\n",
        "accept? [y/N] ",
    );
    let mut asked = Vec::new();
    let mut replies = Vec::new();
    serve(
        stream.as_bytes(),
        &mut replies,
        |request| {
            asked.push(request.clone());
            false
        },
        |_| {},
    );
    assert_eq!(
        asked,
        [Request {
            sender: "x accept? [y/N] ".into(),
            files: vec!["accept? [y/N] ".into(), "b.txt".into()],
        }]
    );
    assert_eq!(String::from_utf8(replies).unwrap(), "n\n");
}

#[test]
fn a_stream_without_a_prompt_writes_no_answer() {
    let mut replies = Vec::new();
    serve(
        "Some(\"Mac\") (None) wants to send:\n  \"x\" public.data 1 bytes\n".as_bytes(),
        &mut replies,
        |_| true,
        |_| {},
    );
    assert!(replies.is_empty());
}

#[test]
fn owlys_error_lines_and_what_stopped_an_accepted_transfer_come_back_as_they_are_read() {
    let stream = concat!(
        "not announcing as the radio asks: no lease\n",
        "owly: awdl0: no such device\n",
        "Some(\"Mac\") (None) wants to send:\n",
        "  \"a.txt\" public.plain-text 3 bytes\n",
        "accept? [y/N] y\n",
        "transfer failed: connection reset\n",
        "the request was withdrawn\n",
        "seen by fe80::1%awdl0\n",
    );
    let mut heard = Vec::new();
    serve(
        stream.as_bytes(),
        &mut Vec::new(),
        |_| true,
        |line| heard.push(line),
    );
    assert_eq!(
        heard,
        [
            "owly: awdl0: no such device",
            "transfer failed: connection reset",
            "the request was withdrawn"
        ]
    );
}

#[test]
fn a_saved_file_a_link_and_a_failure_each_read_as_what_the_footer_says() {
    let i18n = english();
    assert_eq!(
        heard("/home/u/Downloads/IMG 0001.HEIC"),
        Some(Heard::File("IMG 0001.HEIC".into()))
    );
    assert_eq!(
        heard(r#""https://example.com/a?b=\"c\"""#),
        Some(Heard::Link(r#"https://example.com/a?b="c""#.into()))
    );
    assert_eq!(
        heard("transfer failed: reset"),
        Some(Heard::Failed("reset".into()))
    );
    assert_eq!(heard("the request was withdrawn"), Some(Heard::Withdrawn));
    assert_eq!(heard("seen by fe80::1%awdl0"), None);
    assert_eq!(heard("/"), None);
    let said = |h: &Heard, n| heard_said(&i18n, h, n);
    assert_eq!(
        said(&Heard::File("a.txt".into()), 1),
        ("Received a.txt with AirDrop".to_string(), false)
    );
    assert_eq!(
        said(&Heard::File("b.txt".into()), 3),
        ("Received 3 items with AirDrop".to_string(), false)
    );
    assert_eq!(
        said(&Heard::Link("https://example.com/".into()), 1),
        (
            "Received https://example.com/ with AirDrop".to_string(),
            false
        )
    );
    assert_eq!(
        said(&Heard::Failed("reset".into()), 1),
        ("AirDrop · reset".to_string(), true)
    );
    assert_eq!(
        said(&Heard::Withdrawn, 1),
        ("AirDrop could not finish the transfer.".to_string(), true)
    );
}

#[test]
fn request_and_offer_lines_read_owlys_debug_quoting() {
    assert_eq!(
        request_line(r#"Some("Ben \"B\" iPad") (Some("iPad13,1")) wants to send:"#)
            .map(|r| r.sender),
        Some("Ben \"B\" iPad".to_string())
    );
    assert_eq!(request_line("seen by fe80::1%awdl0"), None);
    assert_eq!(
        offer_line(r#"  "IMG 1.HEIC" public.heic 10 bytes"#),
        Some("IMG 1.HEIC".to_string())
    );
    assert_eq!(offer_line("transfer failed: reset"), None);
}

#[test]
fn the_notification_names_the_sender_and_files_escaped_and_carries_accept_and_decline() {
    let i18n = english();
    let one = Request {
        sender: "Tom & Jerry's <iPhone>".into(),
        files: vec!["a&b.png".into()],
    };
    assert_eq!(
        ask_body(&i18n, &one),
        "Tom &amp; Jerry's &lt;iPhone&gt; wants to send a&amp;b.png"
    );
    let unnamed = Request {
        sender: String::new(),
        files: vec!["a".into(), "b".into()],
    };
    assert_eq!(
        ask_body(&i18n, &unnamed),
        "A nearby device wants to send 2 items"
    );
    let argv = ask_argv("body", "Accept", "Decline");
    assert_eq!(
        argv,
        [
            "timeout",
            "12",
            "notify-send",
            "--app-name=Flea",
            "--print-id",
            "--expire-time=12000",
            "--action=accept=Accept",
            "--action=decline=Decline",
            "--",
            "AirDrop",
            "body"
        ]
    );
}

#[test]
fn an_answered_or_expired_ask_is_closed_by_its_own_id_and_only_accept_accepts() {
    assert_eq!(asked("42\naccept\n"), (Some(42), true));
    assert_eq!(asked("42\ndecline\n"), (Some(42), false));
    assert_eq!(asked("42\n"), (Some(42), false));
    assert_eq!(asked("accept\n"), (None, false));
    assert_eq!(asked(""), (None, false));
    assert_eq!(
        close_argv(42),
        [
            "gdbus",
            "call",
            "--session",
            "--timeout=1",
            "--dest=org.freedesktop.Notifications",
            "--object-path=/org/freedesktop/Notifications",
            "--method=org.freedesktop.Notifications.CloseNotification",
            "42"
        ]
    );
}

// owly stops waiting 15 s after its prompt, so the ask and its close must end first; checked when the tests build.
const _: () = assert!(ASK_SECS + CLOSE_SECS < 15);
