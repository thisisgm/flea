// Pipe completion and child exit order these races; watchdogs only bound a missing event.
use super::*;
use std::io::Write;

const DELAYED_OFFER: u32 = INITIAL_OFFER + 1;
const NEXT_TOKEN: &str = "038c038c038c038c038c038c038c038c";
const OWNER_SCRIPT_MODE: u32 = 0o700;

fn gone_owner(reaped: bool) -> Reaped {
    let mut owner = child();
    owner.0.as_mut().unwrap().kill().unwrap();
    if reaped {
        owner.0.as_mut().unwrap().wait().unwrap();
    } else {
        // wait_for_exit returns only once waitid has seen the exit without reaping it.
        crate::clip::own::wait_for_exit(owner.0.as_ref().unwrap().id());
    }
    owner
}

fn fresh_selection(conn: &mut Conn, token: &str, pid: u32) {
    let callback = read_start(conn);
    offer(conn, INITIAL_OFFER, &[format::FLEA, format::GNOME]);
    conn.send(callback, CALLBACK_DONE, &[], &[]).unwrap();
    let answers = HashMap::from([
        (format::FLEA.into(), format!("cut {} {}", token, pid).into_bytes()),
        (format::GNOME.into(), format::build_gnome("cut", &["/tmp/a".into()])),
    ]);
    serve_n(conn, &answers, FLEA_RECEIVES);
}

fn no_more_reads(watching: &mut Watching) {
    watching.finish();
    assert!(watching.incoming.try_recv().is_err(), "no duplicate or stale report");
    watching.listener.set_nonblocking(true).unwrap();
    assert_eq!(watching.listener.accept().unwrap_err().kind(), std::io::ErrorKind::WouldBlock,
        "the completed trigger must not retry its fresh read");
}

#[test]
fn gone_at_report_reads_empty_once() {
    if isolated("races::gone_at_report_reads_empty_once") { return; }
    for reaped in [true, false] {
        let owner = gone_owner(reaped);
        let mut watching = Watching::new();
        watching.report(INITIAL_OFFER, TOKEN, Some(owner.0.as_ref().unwrap().id()));
        let mut fresh = watching.fresh();
        let callback = read_start(&mut fresh);
        empty(&mut fresh, callback);
        assert_eq!(field_str(&changed(&watching.incoming), "clip").as_deref(), Some("none"));
        assert!(fresh.next_raw(WATCHDOG_MS).unwrap().is_none(), "the fresh read completed");
        watching.sentinel(SENTINEL_OFFER);
        no_more_reads(&mut watching);
    }
}

#[test]
fn gone_at_report_reads_new_selection_once() {
    if isolated("races::gone_at_report_reads_new_selection_once") { return; }
    for reaped in [true, false] {
        let owner = gone_owner(reaped);
        let next = child();
        let mut watching = Watching::new();
        watching.report(INITIAL_OFFER, TOKEN, Some(owner.0.as_ref().unwrap().id()));
        let mut fresh = watching.fresh();
        fresh_selection(&mut fresh, NEXT_TOKEN, next.0.as_ref().unwrap().id());
        assert_eq!(field_str(&changed(&watching.incoming), "token").as_deref(), Some(NEXT_TOKEN));
        assert!(fresh.next_raw(WATCHDOG_MS).unwrap().is_none(), "the fresh read completed");
        watching.sentinel(SENTINEL_OFFER);
        no_more_reads(&mut watching);
    }
}

#[test]
fn gone_at_report_dedupes_reserved_token() {
    if isolated("races::gone_at_report_dedupes_reserved_token") { return; }
    for reaped in [true, false] {
        let owner = gone_owner(reaped);
        let mut watching = Watching::new();
        watching.report(INITIAL_OFFER, TOKEN, Some(owner.0.as_ref().unwrap().id()));
        let mut fresh = watching.fresh();
        fresh_selection(&mut fresh, TOKEN, owner.0.as_ref().unwrap().id());
        assert!(fresh.next_raw(WATCHDOG_MS).unwrap().is_none(), "the fresh read completed");
        assert!(watching.incoming.try_recv().is_err(), "a clipboard manager's same token emits nothing");
        watching.sentinel(SENTINEL_OFFER);
        no_more_reads(&mut watching);
    }
}

#[test]
fn foreign_live_pids_do_not_read() {
    if isolated("races::foreign_live_pids_do_not_read") { return; }
    let foreign = Reaped(Some(std::process::Command::new("/bin/cat")
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap()));
    let mut watching = Watching::new();
    watching.report(INITIAL_OFFER, TOKEN, Some(foreign.0.as_ref().unwrap().id()));
    watching.report(DELAYED_OFFER, NEXT_TOKEN, Some(std::process::id()));
    watching.sentinel(SENTINEL_OFFER);
    no_more_reads(&mut watching);
}

#[test]
fn a_delayed_offer_cannot_restore_an_ended_token() {
    if isolated("races::a_delayed_offer_cannot_restore_an_ended_token") { return; }
    let mut owner = child();
    let mut watching = Watching::new();
    watching.report(INITIAL_OFFER, TOKEN, Some(owner.0.as_ref().unwrap().id()));
    let conn = watching.conn.as_mut().unwrap();
    offer(conn, DELAYED_OFFER, &[format::FLEA, format::GNOME]);
    let answers = HashMap::from([(format::FLEA.into(), format!("cut {} {}", TOKEN, owner.0.as_ref().unwrap().id()).into_bytes())]);
    serve_n(conn, &answers, ONE_RECEIVE);
    let request = conn.next_raw(WATCHDOG_MS).unwrap().expect("the delayed GNOME receive");
    assert_eq!((request.sender, request.opcode), (DELAYED_OFFER, OFFER_RECEIVE));
    let mut at = 0;
    assert_eq!(wire::get_string(&request.body, &mut at).as_deref(), Some(format::GNOME));
    let mut delayed = std::fs::File::from(conn.take_fd(WATCHDOG_MS).unwrap().unwrap());
    owner.0.as_mut().unwrap().kill().unwrap();
    let mut fresh = watching.fresh();
    let callback = read_start(&mut fresh);
    empty(&mut fresh, callback);
    assert_eq!(field_str(&changed(&watching.incoming), "clip").as_deref(), Some("none"));
    delayed.write_all(&format::build_gnome("cut", &["/tmp/a".into()])).unwrap();
    drop(delayed);
    watching.sentinel(SENTINEL_OFFER);
    no_more_reads(&mut watching);
}

#[test]
fn a_failed_start_does_not_suppress_the_previous_owners_end() {
    if isolated("races::a_failed_start_does_not_suppress_the_previous_owners_end") { return; }
    let mut owner = child();
    let mut watching = Watching::new();
    watching.report(INITIAL_OFFER, TOKEN, Some(owner.0.as_ref().unwrap().id()));
    let script = watching.dir.executable("failed-owner", b"#!/bin/sh\ncat >/dev/null\n", OWNER_SCRIPT_MODE);
    let result = crate::clip::own::spawn_owner_with(&script, "copy", &["/tmp/new".into()],
        |_| Err(std::sync::mpsc::RecvTimeoutError::Timeout));
    assert_eq!(result.unwrap_err(), "the clipboard owner did not answer");
    owner.0.as_mut().unwrap().kill().unwrap();
    let mut fresh = watching.fresh();
    let callback = read_start(&mut fresh);
    empty(&mut fresh, callback);
    assert_eq!(field_str(&changed(&watching.incoming), "clip").as_deref(), Some("none"));
    assert!(fresh.next_raw(WATCHDOG_MS).unwrap().is_none(), "the previous owner's fresh read completed");
    no_more_reads(&mut watching);
}
