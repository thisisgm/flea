use super::*;
use crate::backend::state::Tables;
use crate::backend::testdir::TestDir;
use crate::backend::thumbs::Pool;
use std::sync::Arc;

struct Rig {
    st: State,
    tb: Tables,
    pool: Pool,
    gen: usize,
}

impl Rig {
    fn open(d: &TestDir, wish: usize) -> Rig {
        let root = d.path().to_str().expect("the sandbox path is utf-8").to_string();
        let tb = Tables::load();
        let (events, _events_rx) = crate::backend::events::channel();
        let mut st = State::new(crate::backend::dirsizeworker::Worker::new(events));
        let (results, _done) = channel();
        let pool = Pool::new(1, results, d.join("cache"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
        st.search_session = Some(Rig::session(&root, wish));
        Rig { st, tb, pool, gen: 0 }
    }

    // A test of the cache never turns on scheduling; the deadline tests set their own budget.
    fn session(root: &str, wish: usize) -> Session {
        let mut session = Session::open_sync(root, root, false);
        session.wish_sizes(Some(wish));
        session.sizes.unhurried();
        session
    }

    // One answered query: its rows line and the folder walks it ran.
    fn ask(&mut self, query: &str) -> (String, usize) {
        self.gen += 1;
        let session = self.st.search_session.as_mut().expect("the session stays open");
        let answer = session.query(self.gen, query, 100).expect("a newer gen answers");
        let mut out = Vec::new();
        crate::backend::searchreq::emit_session_answer(&mut out, &mut self.st, &self.tb, &self.pool, &answer);
        let text = String::from_utf8(out).expect("utf-8");
        let rows = text.lines().find(|line| line.starts_with(r#"{"t":"rows""#)).expect("the answer has a rows line");
        (rows.to_string(), self.sizes().walks)
    }

    fn sizes(&mut self) -> &mut AnswerSizes {
        &mut self.st.search_session.as_mut().expect("the session stays open").sizes
    }
}

fn fixture(tag: &str) -> TestDir {
    let d = TestDir::new(tag);
    d.dir("bench-a");
    d.file("bench-a/one.txt", "0123456789");
    d.dir("bench-b");
    d.file("bench-b/two.txt", "01234");
    d.file("bench.txt", "");
    d
}

// The pair a folder's row carries: its index in this answer and the bytes a whole walk counts.
fn pair(st: &State, d: &TestDir, name: &str) -> String {
    let row = (0..st.listing.len()).find(|&row| st.listing.name(row) == name).expect("the folder is listed");
    format!("[{},{}]", row, walk_until(&d.join(name), Instant::now() + Duration::from_secs(30)).bytes)
}

#[test]
fn an_answer_carries_its_folder_sizes_and_walks_each_folder_once_per_session() {
    let d = fixture("answersizes-carry");
    let mut rig = Rig::open(&d, 40);
    let (rows, walks) = rig.ask("bench");
    assert_eq!(walks, 2, "the first answer walks its two folders: {rows}");
    assert!(rows.contains(&pair(&rig.st, &d, "bench-a")) && rows.contains(&pair(&rig.st, &d, "bench-b")), "{rows}");
    assert!(rows.ends_with("]]}"), "the sizes ride as the last field: {rows}");
    let (rows, walks) = rig.ask("ben");
    assert_eq!(walks, 0, "a later answer takes both from the cache: {rows}");
    assert!(rows.contains(",\"z\":["), "and still carries them: {rows}");
    let session = rig.st.search_session.as_mut().expect("the session stays open");
    assert!(session.patch_after_write(&d.join("bench-a/one.txt")));
    let (_, walks) = rig.ask("bench");
    assert_eq!(walks, 2, "a write under the scope forgets every size");
}

#[test]
fn the_open_folders_watch_and_a_reopened_session_forget_every_size() {
    let d = fixture("answersizes-reopen");
    let mut rig = Rig::open(&d, 40);
    assert_eq!(rig.ask("bench").1, 2, "the first answer walks both folders");
    assert_eq!(rig.ask("ben").1, 0, "the next takes both from the cache");
    let session = rig.st.search_session.as_mut().expect("the session stays open");
    assert!(session.patch_open_dir(&d.join("bench-b")));
    assert_eq!(rig.ask("bench").1, 2, "a change the watch reports forgets every size");
    let root = d.path().to_str().expect("the sandbox path is utf-8").to_string();
    rig.st.search_session = Some(Rig::session(&root, 40));
    assert_eq!(rig.ask("ben").1, 2, "a session opened again knows nothing");
}

#[test]
fn a_scope_that_may_not_be_walked_answers_from_what_the_client_asked() {
    let d = fixture("answersizes-remote");
    let mut rig = Rig::open(&d, 40);
    // What the session's own open builds for a remote scope.
    *rig.sizes() = AnswerSizes::new(true);
    rig.sizes().wish = 40;
    let (rows, walks) = rig.ask("bench");
    assert_eq!(walks, 0, "a remote scope walks nothing inline");
    assert!(!rows.contains("\"z\""), "and carries no size it does not know: {rows}");
    let row = (0..rig.st.listing.len()).find(|&row| rig.st.listing.name(row) == "bench-a").expect("bench-a is listed");
    learn(&mut rig.st, row, DirSize { bytes: 5, partial: true });
    let past = rig.st.listing.len();
    learn(&mut rig.st, past, DirSize { bytes: 5, partial: false });
    assert!(rig.sizes().known.is_empty(), "a partial size, or a row past the listing, teaches nothing");
    learn(&mut rig.st, row, DirSize { bytes: 77, partial: false });
    let (rows, walks) = rig.ask("ben");
    assert_eq!(walks, 0);
    assert!(rows.contains(",77]"), "a size the client asked for rides on the next answer: {rows}");
}

#[test]
fn a_walk_that_misses_the_budget_ends_inline_walks_for_the_session() {
    let d = fixture("answersizes-slow");
    let mut rig = Rig::open(&d, 40);
    // What walk_until answers for a folder still walking at the deadline.
    rig.sizes().walk = |_, _| DirSize { bytes: 1, partial: true };
    let (rows, walks) = rig.ask("bench");
    assert_eq!(walks, 1, "the first folder ran out of time and the walk stopped there");
    assert!(!rows.contains("\"z\""), "a missed folder draws what a listing draws: {rows}");
    rig.sizes().walk = walk_until;
    let (_, walks) = rig.ask("ben");
    assert_eq!(walks, 0, "a slow scope pays the budget once");
}

#[test]
fn a_walk_that_blocks_never_holds_the_answer_past_the_budget() {
    let d = fixture("answersizes-blocked");
    let mut rig = Rig::open(&d, 40);
    // Long enough that the walk thread has surely begun, far short of the walk it blocks in.
    rig.sizes().budget = Duration::from_millis(500);
    rig.sizes().walk = |_, _| {
        std::thread::sleep(Duration::from_secs(5));
        DirSize { bytes: 1, partial: false }
    };
    let began = Instant::now();
    let (rows, walks) = rig.ask("bench");
    assert!(began.elapsed() < Duration::from_secs(2), "the blocked walk holds its own thread, not the answer");
    assert_eq!(walks, 0, "no folder came back inside the budget");
    assert!(!rows.contains("\"z\""), "{rows}");
    assert!(!rig.sizes().walkable, "and the scope walks nothing more");
}

#[test]
fn a_query_without_a_wish_keeps_the_one_before() {
    let d = fixture("answersizes-wish");
    let mut rig = Rig::open(&d, 40);
    rig.st.search_session.as_mut().expect("open").wish_sizes(None);
    assert_eq!(rig.sizes().wish, 40, "a re-rendered burst query keeps the wish");
    rig.st.search_session.as_mut().expect("open").wish_sizes(Some(0));
    let (rows, walks) = rig.ask("bench");
    assert_eq!(walks, 0, "a hidden Size column asks for nothing");
    assert!(!rows.contains("\"z\""), "{rows}");
    let parsed = crate::backend::proto::parse_request(r#"{"c":"searchquery","query":"a","gen":1,"sizes":12}"#);
    assert!(matches!(parsed, crate::backend::proto::Request::SearchQuery { sizes: Some(12), .. }));
    let parsed = crate::backend::proto::parse_request(r#"{"c":"searchquery","query":"a","gen":1}"#);
    assert!(matches!(parsed, crate::backend::proto::Request::SearchQuery { sizes: None, .. }));
}

#[test]
fn only_complete_sizes_inside_the_window_ride_in_row_order() {
    let mut dirsizes = HashMap::new();
    dirsizes.insert(2, (9, false));
    dirsizes.insert(0, (40, false));
    dirsizes.insert(1, (5, true));
    dirsizes.insert(7, (3, false));
    let line = with_sizes(r#"{"t":"rows","start":0,"rows":[],"gen":3}"#.to_string(), &dirsizes, 0..3);
    assert_eq!(line, r#"{"t":"rows","start":0,"rows":[],"gen":3,"z":[[0,40],[2,9]]}"#);
    let line = with_sizes(r#"{"t":"rows"}"#.to_string(), &HashMap::new(), 0..3);
    assert_eq!(line, r#"{"t":"rows"}"#, "no known size adds no field");
}

#[test]
fn a_window_past_the_head_fills_its_own_folder_rows() {
    let d = fixture("answersizes-start");
    let mut rig = Rig::open(&d, 40);
    rig.ask("bench");
    let deep = (0..rig.st.listing.len()).rev().find(|&row| rig.st.listing.is_dir(row)).expect("a folder is listed");
    assert!(deep > 0, "the later folder sits past the first row");
    rig.st.dirsizes.clear();
    fill(&mut rig.st, deep, 1);
    let mut keys: Vec<usize> = rig.st.dirsizes.keys().copied().collect();
    keys.sort();
    assert_eq!(keys, vec![deep], "a window starting at a later row fills that row, not the head's");
}

// A short answer's arena is sized to its rows, under glibc's 128 KiB mmap threshold, so the answer maps no 1 MiB block.
#[test]
fn answer_listing_is_sized_to_its_rows() {
    let d = fixture("answersizes-arena");
    let mut rig = Rig::open(&d, 40);
    rig.ask("bench");
    assert!(rig.st.listing.len() >= 3, "{}", rig.st.listing.len());
    assert!(rig.st.listing.names.capacity() < 4096, "names capacity {}", rig.st.listing.names.capacity());
    assert!(rig.st.listing.spans.capacity() < 64, "spans capacity {}", rig.st.listing.spans.capacity());
}

// Issue 390: a size the settled cache answers teaches the session, as a walked one does.
#[test]
fn a_settled_size_the_client_asked_for_teaches_the_session() {
    let d = fixture("answersizes-settled");
    let mut rig = Rig::open(&d, 40);
    *rig.sizes() = AnswerSizes::new(true);
    rig.sizes().wish = 40;
    rig.ask("bench");
    let row = (0..rig.st.listing.len())
        .find(|&row| rig.st.listing.name(row) == "bench-a")
        .expect("bench-a is listed");
    let path = rig.st.base.join("bench-a");
    let watch = crate::backend::dirsizewatch::TreeWatch::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    let (size, pins) =
        crate::backend::dirsizewatch::walk_watched(&path, deadline, &|| false, &watch);
    rig.st.settled.settle(path, size.bytes, pins);
    rig.st.dirsizes.remove(&row);
    let mut out = Vec::new();
    crate::backend::dirsizereq::queue_dirsizes(&mut out, &mut rig.st, &[row]);
    assert!(!out.is_empty(), "the settled size answers at once");
    let (rows, walks) = rig.ask("ben");
    assert_eq!(walks, 0, "a remote scope walks nothing inline");
    let pair = format!(",{}]", size.bytes);
    assert!(
        rows.contains(&pair),
        "the settled size rides on the next answer: {rows}"
    );
}
