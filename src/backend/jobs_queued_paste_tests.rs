//! Queued paste operations retain independent on-disk effects and undo entries.
use super::super::{jobs::State, opsdispatch, opsreq::OpMsg, testdir::TestDir};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

fn settle(ops: &mut opsdispatch::Ops, rx: &Receiver<OpMsg>, out: &mut Vec<u8>) {
    while ops.jobs.unfinished() > 0 {
        let reply = rx
            .recv_timeout(Duration::from_secs(30))
            .expect("both queued jobs finish");
        opsdispatch::report_op(out, ops, reply);
    }
}

#[test]
fn queued_paste_moves_and_copies_each_undo_newest_first() {
    for op in ["move", "copy"] {
        let root = TestDir::new("queued-paste-undo");
        let a = root.file("a", "first payload");
        let b = root.file("b", "second payload");
        let dest = root.dir("destination");
        let (tx, rx) = channel();
        let mut ops = opsdispatch::Ops::new(tx);
        let mut out = Vec::new();
        for path in [&a, &b] {
            opsdispatch::start_transfer(
                &mut out,
                &mut ops,
                op,
                vec![path.to_str().unwrap().into()],
                dest.to_str().unwrap(),
                Default::default(),
            );
        }
        assert_eq!(ops.jobs.unfinished(), 2);
        settle(&mut ops, &rx, &mut out);
        assert!(ops.jobs.rows.iter().all(|job| job.state == State::Done));
        assert_eq!(
            std::fs::read_to_string(dest.join("a")).unwrap(),
            "first payload"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("b")).unwrap(),
            "second payload"
        );
        if op == "move" {
            assert!(!a.exists() && !b.exists());
        }
        for name in ["b", "a"] {
            out.clear();
            opsdispatch::do_undo(&mut out, &mut ops);
            let reply = String::from_utf8_lossy(&out);
            assert!(reply.contains(r#""t":"undone""#), "{reply}");
            assert!(!dest.join(name).exists());
            if name == "b" {
                assert!(
                    dest.join("a").exists(),
                    "undoing b preserves a's completed transfer"
                );
            }
        }
        assert_eq!(std::fs::read_to_string(a).unwrap(), "first payload");
        assert_eq!(std::fs::read_to_string(b).unwrap(), "second payload");
    }
}

// A total-0 question sends no choice, so the second paste's name meets the exclusive create, never a replace.
#[test]
fn queued_paste_onto_a_name_the_first_landed_fails_that_item_and_never_overwrites() {
    let root = TestDir::new("queued-paste-collide");
    let (one, two) = (root.dir("one"), root.dir("two"));
    let dest = root.dir("destination");
    std::fs::write(one.join("a"), "first payload").unwrap();
    std::fs::write(two.join("a"), "second payload").unwrap();
    let (tx, rx) = channel();
    let mut ops = opsdispatch::Ops::new(tx);
    let mut out = Vec::new();
    for dir in [&one, &two] {
        opsdispatch::start_transfer(
            &mut out,
            &mut ops,
            "copy",
            vec![dir.join("a").to_str().unwrap().into()],
            dest.to_str().unwrap(),
            Default::default(),
        );
    }
    settle(&mut ops, &rx, &mut out);
    let reply = String::from_utf8_lossy(&out);
    assert_eq!(
        std::fs::read_to_string(dest.join("a")).unwrap(),
        "first payload"
    );
    assert!(reply.contains(r#""ok":false"#), "{reply}");
    assert!(
        reply.contains("already exists"),
        "EEXIST refuses it: {reply}"
    );
    assert!(
        !reply.contains(super::super::opsreq::ALREADY_THERE),
        "{reply}"
    );
    assert!(ops.jobs.rows.iter().any(|job| job.state == State::Failed));
    assert_eq!(
        std::fs::read_to_string(two.join("a")).unwrap(),
        "second payload"
    );
}
