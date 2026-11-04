use super::*;
use crate::backend::testdir::TestDir;
use crate::backend::thumbspec::Thumbnailers;
use crate::backend::trashdelete::Reviewed;
use crate::json::{escape, field_str, field_usize};
use std::os::unix::fs::MetadataExt;

fn wire(line: &str, st: &mut State, tb: &Tables, pool: &Pool, cache: &Cache) -> Vec<u8> {
    let (events, _receiver) = crate::backend::events::channel();
    let mut ops = Ops::new(channel().0);
    let mut flash = flashslot::Ctx::new(channel().0);
    let mut watch = Watch::start(events.clone());
    let mut peeks = PeekWatch::start(events.clone());
    let mut fsinfo = FsInfo::new(events.clone());
    let poller = super::super::watchpoll::Poller::new(events.clone());
    let mut gate = Gate::new(events.clone());
    let mut out = Vec::new();
    assert!(
        handle_line(
            line,
            &mut out,
            st,
            tb,
            pool,
            cache,
            &mut ops,
            &mut flash,
            &mut watch,
            &mut peeks,
            &mut fsinfo,
            &poller,
            &events,
            &mut gate
        ) == Control::Continue
    );
    out
}

fn line(d: &TestDir, name: &str, id: usize) -> String {
    let path = d.file(&format!("files/{name}"), "image bytes");
    d.file(
        &format!("info/{name}.trashinfo"),
        "[Trash Info]\nPath=/original/photo.jpg\n",
    );
    let meta = path.symlink_metadata().unwrap();
    let reviewed =
        Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    format!(
        r#"{{"c":"trashthumb","id":{id},"original":"/original/photo.jpg","path":"{}","provider":"l{}:{}","identity":"{}"}}"#,
        escape(&path.to_string_lossy()),
        meta.dev(),
        meta.ino(),
        escape(&reviewed.selection_identity())
    )
}

fn fixture(d: &TestDir) -> (State, Tables, Pool, Cache) {
    d.dir("files");
    d.dir("info");
    let mut tb = Tables::load();
    tb.thumbs = Arc::new(Thumbnailers::from_entries(
        &[(
            "t.thumbnailer".into(),
            "[Thumbnailer Entry]\nExec=/usr/bin/false %i %o\nMimeType=image/jpeg;\n".into(),
        )],
        &tb.aliases,
    )).into();
    let st = State::new(super::super::dirsizeworker::Worker::new(crate::backend::events::channel().0));
    (st, tb, Pool::idle(), Cache::at(d.join("cache")))
}

#[test]
fn advloop_f1_protocol_refuses_missing_id_without_consuming_explicit_zero() {
    let d = TestDir::new("trashthumb-no-id");
    let (mut st, tb, pool, cache) = fixture(&d);
    let line = line(&d, "one.jpg", 0);
    let missing = line.replacen("\"id\":0,", "", 1);
    assert!(wire(&missing, &mut st, &tb, &pool, &cache).is_empty());
    assert_eq!(
        (pool.pending(), st.outstanding, st.trash_asked.len()),
        (0, 0, 0),
        "an id-less request must not enter the queue"
    );
    assert!(wire(&line, &mut st, &tb, &pool, &cache).is_empty());
    assert_eq!(
        (pool.pending(), st.outstanding, st.trash_asked.len()),
        (1, 1, 1),
        "explicit zero is a real caller identity"
    );
}

#[test]
fn advloop_f1_protocol_reuses_cancelled_id_and_shares_the_running_job() {
    let d = TestDir::new("trashthumb-reuse-id");
    let (mut st, tb, pool, cache) = fixture(&d);
    let line = line(&d, "one.jpg", 7);
    assert!(wire(&line, &mut st, &tb, &pool, &cache).is_empty());
    let running = pool.cancel_all().pop().unwrap();
    assert!(wire(
        r#"{"c":"trashthumbcancel","ids":[7]}"#,
        &mut st,
        &tb,
        &pool,
        &cache
    )
    .is_empty());
    assert_eq!(
        st.trash_asked.len(),
        1,
        "a running cancelled ask is retained until completion"
    );
    assert!(wire(&line, &mut st, &tb, &pool, &cache).is_empty());
    assert_eq!(
        (pool.pending(), st.outstanding),
        (0, 1),
        "the fresh subscriber joins the same running job"
    );
    let mut out = Vec::new();
    let file = d.join("cache/ready.png");
    super::super::thumbreq::report_done(
        &mut out,
        &mut st,
        &pool,
        super::super::thumbs::Done { replies: None,
            path: running.path,
            mtime: running.mtime,
            trash_identity: running.trash_identity,
            result: super::super::thumbs::Outcome::Ready(file.clone()),
            ms: 0.0,
            trace: None,
        },
    );
    let out = String::from_utf8(out).unwrap();
    assert_eq!(
        out.lines().count(),
        1,
        "the reused id must receive its one completion"
    );
    assert_eq!(field_usize(&out, "id"), Some(7));
    assert_eq!(
        field_str(&out, "file"),
        Some(file.to_string_lossy().into_owned())
    );
    assert_eq!(st.outstanding, 0);
}

#[test]
fn advloop_f1_protocol_keeps_live_duplicates_and_requeues_a_different_cancelled_target() {
    let d = TestDir::new("trashthumb-id-target");
    let (mut st, tb, pool, cache) = fixture(&d);
    let first = line(&d, "one.jpg", 7);
    let second = line(&d, "two.jpg", 7);
    wire(&first, &mut st, &tb, &pool, &cache);
    assert!(wire(&second, &mut st, &tb, &pool, &cache).is_empty());
    assert_eq!(
        (pool.pending(), st.outstanding, st.trash_asked.len()),
        (1, 1, 1),
        "a live duplicate cannot replace its target"
    );
    let old = pool.cancel_all().pop().unwrap();
    wire(
        r#"{"c":"trashthumbcancel","ids":[7]}"#,
        &mut st,
        &tb,
        &pool,
        &cache,
    );
    wire(&second, &mut st, &tb, &pool, &cache);
    assert_eq!(
        (pool.pending(), st.outstanding, st.trash_asked.len()),
        (1, 2, 1),
        "a different target needs its own job and one fresh mapping"
    );
    let new = pool.cancel_all().pop().unwrap();
    assert_ne!(old.path, new.path);
    let mut out = Vec::new();
    super::super::thumbreq::report_done(
        &mut out,
        &mut st,
        &pool,
        super::super::thumbs::Done { replies: None,
            path: old.path,
            mtime: old.mtime,
            trash_identity: old.trash_identity,
            result: super::super::thumbs::Outcome::Ready(d.join("cache/one.jpg")),
            ms: 0.0,
            trace: None,
        },
    );
    assert_eq!(st.outstanding, 1);
    assert!(
        out.is_empty(),
        "the cancelled target must not answer the reused id"
    );
    let file = d.join("cache/two.jpg");
    super::super::thumbreq::report_done(
        &mut out,
        &mut st,
        &pool,
        super::super::thumbs::Done { replies: None,
            path: new.path,
            mtime: new.mtime,
            trash_identity: new.trash_identity,
            result: super::super::thumbs::Outcome::Ready(file.clone()),
            ms: 0.0,
            trace: None,
        },
    );
    let out = String::from_utf8(out).unwrap();
    assert_eq!(out.lines().count(), 1);
    assert_eq!(field_usize(&out, "id"), Some(7));
    assert_eq!(
        field_str(&out, "file"),
        Some(file.to_string_lossy().into_owned())
    );
    assert_eq!(st.outstanding, 0);
}
