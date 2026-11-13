// Hold battery: no frame between a listed and its rows, capped like the GUI swap, with rows, parent column and first preview body in one frame.
use super::hold::Hold;
use super::*;
use crate::jsondoc;
use crate::tui::model::Model;
use std::time::{Duration, Instant};
// The loop's paint decision through the hold under test.
fn paint_with(hold: &Hold, model: &Model, now: Instant, launched: Instant) -> bool {
    if hold.hold(model, now, launched) {
        return false;
    }
    let theme = theme::Theme::load();
    let map = keymap::Map::load();
    let mut frame = String::new();
    render::draw(model, &theme, &map, render::TerminalSize { columns: 80, lines: 24 }, now.duration_since(launched), &mut frame).unwrap()
}
// A listing as the loop sees it: request observed, then listed drained, with the model noting its own peek send.
fn listed_model(hold: &mut Hold, total: usize, launched: Instant) -> (Model, crate::tui::wire::Wire, std::thread::JoinHandle<()>) {
    let (mut wire, reader) = crate::tui::wire::echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &jsondoc::Json::Null);
    model.open(PathBuf::from("/listing"), &mut wire).unwrap();
    hold.sync(&model, launched);
    model
        .receive(jsondoc::parse(&format!(r#"{{"t":"listed","n":{}}}"#, total)).unwrap(), &mut wire)
        .unwrap();
    (model, wire, reader)
}
fn rows_landed(model: &mut Model, wire: &mut crate::tui::wire::Wire) {
    model
        .receive(jsondoc::parse(r#"{"t":"rows","start":0,"rows":[{"n":"a.txt"}]}"#).unwrap(), wire)
        .unwrap();
}
// A listing at an explicit path: request, listed, model notes its peek.
fn path_listed(hold: &mut Hold, model: &mut Model, wire: &mut crate::tui::wire::Wire, path: &str, total: usize, at: Instant) {
    model.open(PathBuf::from(path), wire).unwrap();
    hold.sync(model, at);
    model
        .receive(jsondoc::parse(&format!(r#"{{"t":"listed","n":{}}}"#, total)).unwrap(), wire)
        .unwrap();
}
// A parent peek reply as the backend sends it; the model stores the rows.
fn peek_landed(model: &mut Model, wire: &mut crate::tui::wire::Wire, path: &str) {
    model
        .receive(jsondoc::parse(&format!(r#"{{"t":"peeked","path":"{}","rows":[{{"n":"p"}}]}}"#, path)).unwrap(), wire)
        .unwrap();
}
// A parent peek that fails still answers (peek.rs:24,39), so the hold ends on the reply instead of the cap.
fn failed_peek_landed(model: &mut Model, wire: &mut crate::tui::wire::Wire, path: &str) {
    model
        .receive(jsondoc::parse(&format!(r#"{{"t":"peeked","path":"{}","n":0,"failed":true,"rows":[]}}"#, path)).unwrap(), wire)
        .unwrap();
}
// A pending listing older than the cap draws.
#[test]
fn capped_hold_draws_a_slow_listing() {
    let mut hold = Hold::new();
    hold.mark_painted();
    let launched = Instant::now();
    let (model, wire, reader) = listed_model(&mut hold, 5, launched);
    assert!(!paint_with(&hold, &model, launched + Duration::from_millis(100), launched));
    assert!(paint_with(&hold, &model, launched + Duration::from_millis(200), launched));
    crate::tui::wire::finish(wire, reader);
}
// Rows hold for their parent column and paint with it.
#[test]
fn rows_wait_for_the_parent_column() {
    let mut hold = Hold::new();
    hold.mark_painted();
    let launched = Instant::now();
    let (mut model, mut wire, reader) = listed_model(&mut hold, 5, launched);
    rows_landed(&mut model, &mut wire);
    assert!(!paint_with(&hold, &model, launched, launched));
    peek_landed(&mut model, &mut wire, "/");
    assert!(paint_with(&hold, &model, launched, launched));
    crate::tui::wire::finish(wire, reader);
}
// The first screen holds while the reader owes the cursor's text body, and paints once the loop takes it.
#[test]
fn first_screen_waits_for_the_preview_body() {
    let mut hold = Hold::new();
    let launched = Instant::now();
    let (mut model, mut wire, reader) = listed_model(&mut hold, 5, launched);
    model.preview_visible = true;
    model
        .receive(jsondoc::parse(r#"{"t":"rows","start":0,"rows":[{"n":"a.txt","i":"text-plain"}]}"#).unwrap(), &mut wire)
        .unwrap();
    peek_landed(&mut model, &mut wire, "/");
    preview::load(&mut model, true);
    assert!(model.preview_reader.outstanding(model.preview_generation), "the text read never went out");
    assert!(!paint_with(&hold, &model, launched, launched), "the first screen drew before its preview body");
    let deadline = Instant::now() + Duration::from_secs(2);
    while model.preview_reader.outstanding(model.preview_generation) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
        preview::poll(&mut model);
    }
    assert!(paint_with(&hold, &model, launched, launched), "the first screen held after its preview body landed");
    crate::tui::wire::finish(wire, reader);
}
// A landed body whose file is named like the placeholder paints the first screen at once.
#[test]
fn a_landed_preview_named_like_the_placeholder_paints() {
    let mut hold = Hold::new();
    let launched = Instant::now();
    let (mut model, mut wire, reader) = listed_model(&mut hold, 5, launched);
    rows_landed(&mut model, &mut wire);
    peek_landed(&mut model, &mut wire, "/");
    model.preview = vec!["Loading…".into(), "text · 0 B".into(), String::new()];
    assert!(paint_with(&hold, &model, launched, launched), "a landed preview held the first paint");
    crate::tui::wire::finish(wire, reader);
}
// Every reply the echo wire returned up to a fence, so a test sees each request a key sent.
fn sent(wire: &mut crate::tui::wire::Wire) -> Vec<jsondoc::Json> {
    wire.send(vec![("c", crate::tui::wire::word("fence"))]).unwrap();
    let mut out = Vec::new();
    loop {
        let value = wire.events.recv_timeout(Duration::from_secs(2)).expect("the echo wire never returned its fence").unwrap();
        if crate::tui::wire::text(&value, "c") == "fence" {
            return out;
        }
        out.push(value);
    }
}
fn trashes(wire: &mut crate::tui::wire::Wire) -> usize {
    sent(wire).iter().filter(|value| crate::tui::wire::text(value, "c") == "trash").count()
}
// A held frame takes keys as a listing still out: a trash key on rows not drawn yet sends nothing.
#[test]
fn held_frame_swallows_a_trash_key() {
    let mut hold = Hold::new();
    hold.mark_painted();
    let launched = Instant::now();
    let (mut model, mut wire, reader) = listed_model(&mut hold, 5, launched);
    rows_landed(&mut model, &mut wire);
    sent(&mut wire);
    assert!(model.pending.is_none() && model.peek_out(), "the rows landed and the parent peek is out");
    let delete = input::Key::named("Delete", "");
    assert!(matches!(keymap::Map::load().action(&delete, &model.preset).as_str(), "trash" | "trashArm"));
    model.held = hold.hold(&model, launched, launched);
    assert!(model.held, "the frame is held for the parent column");
    press_twice(&mut model, &mut wire, &delete);
    let out = sent(&mut wire);
    assert!(out.is_empty(), "the held frame sent {} request(s) for keys on rows it never drew", out.len());
    peek_landed(&mut model, &mut wire, "/");
    model.held = hold.hold(&model, launched, launched);
    assert!(!model.held);
    press_twice(&mut model, &mut wire, &delete);
    assert!(trashes(&mut wire) > 0, "once drawn, the trash key trashes the row under the cursor");
    crate::tui::wire::finish(wire, reader);
}
// The key goes to the model the way the loop sends it, twice so an arm-then-confirm trash still reaches the wire.
fn press_twice(model: &mut Model, wire: &mut crate::tui::wire::Wire, key: &input::Key) {
    for _ in 0..2 {
        actions::key(model, key, &keymap::Map::load(), wire).unwrap();
    }
}
// A frame held only for the first preview body, its parent peek already landed, takes keys as a listing still out too.
#[test]
fn a_frame_held_for_the_preview_body_swallows_a_trash_key() {
    let mut hold = Hold::new();
    let launched = Instant::now();
    let (mut model, mut wire, reader) = listed_model(&mut hold, 5, launched);
    model.preview_visible = true;
    model
        .receive(jsondoc::parse(r#"{"t":"rows","start":0,"rows":[{"n":"a.txt","i":"text-plain"}]}"#).unwrap(), &mut wire)
        .unwrap();
    peek_landed(&mut model, &mut wire, "/");
    preview::load(&mut model, true);
    sent(&mut wire);
    assert!(model.pending.is_none() && !model.peek_out(), "the rows and the parent peek landed");
    assert!(model.preview_reader.outstanding(model.preview_generation), "the text read never went out");
    let delete = input::Key::named("Delete", "");
    model.held = hold.hold(&model, launched, launched);
    assert!(model.held, "the frame is held for the preview body alone");
    press_twice(&mut model, &mut wire, &delete);
    assert_eq!(trashes(&mut wire), 0, "a trash key acted on a row the held frame never drew");
    let deadline = Instant::now() + Duration::from_secs(2);
    while model.preview_reader.outstanding(model.preview_generation) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
        preview::poll(&mut model);
    }
    model.held = hold.hold(&model, launched, launched);
    assert!(!model.held, "the preview body landed");
    press_twice(&mut model, &mut wire, &delete);
    assert!(trashes(&mut wire) > 0, "once drawn, the trash key trashes the row under the cursor");
    crate::tui::wire::finish(wire, reader);
}
// A settled empty directory draws at once, never held.
#[test]
fn settled_empty_draws() {
    let mut hold = Hold::new();
    let launched = Instant::now();
    let (model, wire, reader) = listed_model(&mut hold, 0, launched);
    assert!(paint_with(&hold, &model, launched, launched));
    crate::tui::wire::finish(wire, reader);
}
// Rows with their peek and a settled preview draw.
#[test]
fn landed_all_draws() {
    let mut hold = Hold::new();
    hold.mark_painted();
    let launched = Instant::now();
    let (mut model, mut wire, reader) = listed_model(&mut hold, 5, launched);
    rows_landed(&mut model, &mut wire);
    peek_landed(&mut model, &mut wire, "/");
    assert!(paint_with(&hold, &model, launched, launched));
    crate::tui::wire::finish(wire, reader);
}
// A search in flight is not a listing and draws through.
#[test]
fn searching_draws_through() {
    let mut hold = Hold::new();
    hold.mark_painted();
    let launched = Instant::now();
    let (mut model, wire, reader) = listed_model(&mut hold, 5, launched);
    model.searching = true;
    assert!(paint_with(&hold, &model, launched, launched));
    crate::tui::wire::finish(wire, reader);
}
// The wait budget is the hold deadline, not the quantum.
#[test]
fn wait_budget_is_the_hold_deadline() {
    let mut hold = Hold::new();
    let launched = Instant::now();
    let (model, wire, reader) = listed_model(&mut hold, 5, launched);
    assert_eq!(hold.wait_ms(&model, launched + Duration::from_millis(110), launched, 100), 40);
    crate::tui::wire::finish(wire, reader);
}
// The loop's wait on a held listing ends on the hold deadline, not up to a quantum late, with stdin and the wake both silent.
#[test]
fn fallback_lands_on_the_deadline() {
    use std::os::unix::io::AsRawFd;
    let (stdin, _held) = std::os::unix::net::UnixStream::pair().unwrap();
    let (wake, _signaler) = std::os::unix::net::UnixStream::pair().unwrap();
    let mut hold = Hold::new();
    hold.mark_painted();
    let (mut wire, reader) = crate::tui::wire::echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &jsondoc::Json::Null);
    model.open(PathBuf::from("/listing"), &mut wire).unwrap();
    let requested = Instant::now().checked_sub(Duration::from_millis(110)).unwrap();
    hold.sync(&model, requested);
    model.receive(jsondoc::parse(r#"{"t":"listed","n":5}"#).unwrap(), &mut wire).unwrap();
    let budget = hold.wait_ms(&model, Instant::now(), requested, 100);
    // The budget is read off the hold deadline, never the wall clock: a quantum-sized wait reads 100 here.
    assert!(budget <= 40, "the wait budget is {}, not the hold deadline", budget);
    assert!(!crate::tui::terminal::wait_input(stdin.as_raw_fd(), wake.as_raw_fd(), budget).unwrap());
    let landed = requested.elapsed();
    assert!(landed >= Duration::from_millis(150), "the wait ended at {:?}, before the hold deadline", landed);
    crate::tui::wire::finish(wire, reader);
}
// A readable stdin answers true at once, long before the timeout.
#[test]
fn readable_stdin_answers_true() {
    use std::io::Write;
    use std::os::unix::io::AsRawFd;
    let (stdin, mut held) = std::os::unix::net::UnixStream::pair().unwrap();
    let (wake, _signaler) = std::os::unix::net::UnixStream::pair().unwrap();
    held.write_all(b"j").unwrap();
    let before = Instant::now();
    assert!(crate::tui::terminal::wait_input(stdin.as_raw_fd(), wake.as_raw_fd(), 5000).unwrap());
    assert!(before.elapsed() < Duration::from_millis(1000), "a readable stdin waited {:?}", before.elapsed());
}
// A wake byte with stdin silent ends a blocked wait early and answers false, so the loop drains the wire before reading.
#[test]
fn signaled_wake_returns_before_the_timeout() {
    use std::io::Write;
    use std::os::unix::io::AsRawFd;
    let (stdin, _held) = std::os::unix::net::UnixStream::pair().unwrap();
    let (wake, mut signaler) = std::os::unix::net::UnixStream::pair().unwrap();
    // The clock starts before the ring thread, so a wait that blocked until the ring lasts at least RING.
    const RING: Duration = Duration::from_millis(20);
    let before = Instant::now();
    let ring = std::thread::spawn(move || {
        std::thread::sleep(RING);
        signaler.write_all(&[1u8]).unwrap();
        signaler
    });
    assert!(!crate::tui::terminal::wait_input(stdin.as_raw_fd(), wake.as_raw_fd(), 5000).unwrap());
    let waited = before.elapsed();
    assert!(waited >= RING && waited < Duration::from_millis(1000), "a rung wake waited {:?}", waited);
    drop(ring.join().unwrap());
}
// Launch at / paints when rows land: no parent exists, so no peek for this listing can be outstanding.
#[test]
fn root_launch_paints_with_rows() {
    let mut hold = Hold::new();
    let launched = Instant::now();
    let (mut echo, handle) = crate::tui::wire::echo_wire();
    let mut model = Model::new(PathBuf::from("/"), &jsondoc::Json::Null);
    path_listed(&mut hold, &mut model, &mut echo, "/", 3, launched);
    assert!(!model.peek_out());
    rows_landed(&mut model, &mut echo);
    assert!(paint_with(&hold, &model, launched, launched));
    crate::tui::wire::finish(echo, handle);
}
// A sibling step re-peeks the parent it sits under, so the rows wait for the new parent column and paint with it.
#[test]
fn sibling_step_repeeks_and_paints_on_it() {
    let mut hold = Hold::new();
    let launched = Instant::now();
    let (mut echo, handle) = crate::tui::wire::echo_wire();
    let mut model = Model::new(PathBuf::from("/a/b"), &jsondoc::Json::Null);
    path_listed(&mut hold, &mut model, &mut echo, "/a/b", 2, launched);
    rows_landed(&mut model, &mut echo);
    peek_landed(&mut model, &mut echo, "/a");
    path_listed(&mut hold, &mut model, &mut echo, "/a/c", 2, launched);
    assert!(model.peek_out());
    rows_landed(&mut model, &mut echo);
    assert!(!paint_with(&hold, &model, launched, launched));
    peek_landed(&mut model, &mut echo, "/a");
    assert!(!model.peek_out());
    assert!(paint_with(&hold, &model, launched, launched));
    crate::tui::wire::finish(echo, handle);
}
// A same-path relist re-peeks its parent, so the rows wait for the fresh parent column like any other listing.
#[test]
fn relist_repeeks_and_waits_for_it() {
    let mut hold = Hold::new();
    hold.mark_painted();
    let launched = Instant::now();
    let (mut echo, handle) = crate::tui::wire::echo_wire();
    let mut model = Model::new(PathBuf::from("/p"), &jsondoc::Json::Null);
    path_listed(&mut hold, &mut model, &mut echo, "/p", 2, launched);
    rows_landed(&mut model, &mut echo);
    peek_landed(&mut model, &mut echo, "/");
    path_listed(&mut hold, &mut model, &mut echo, "/p", 2, launched);
    assert!(model.peek_out());
    rows_landed(&mut model, &mut echo);
    assert!(!paint_with(&hold, &model, launched, launched));
    peek_landed(&mut model, &mut echo, "/");
    assert!(paint_with(&hold, &model, launched, launched));
    crate::tui::wire::finish(echo, handle);
}
// A hidden toggle or a sort relist re-peeks, so the rows never keep a parent column fetched under the old setting.
#[test]
fn hidden_toggle_and_sort_relist_repeek() {
    let mut hold = Hold::new();
    hold.mark_painted();
    let launched = Instant::now();
    let (mut echo, handle) = crate::tui::wire::echo_wire();
    let mut model = Model::new(PathBuf::from("/p"), &jsondoc::Json::Null);
    path_listed(&mut hold, &mut model, &mut echo, "/p", 2, launched);
    rows_landed(&mut model, &mut echo);
    peek_landed(&mut model, &mut echo, "/");
    model.hidden = true;
    path_listed(&mut hold, &mut model, &mut echo, "/p", 2, launched);
    assert!(model.peek_out());
    rows_landed(&mut model, &mut echo);
    assert!(!paint_with(&hold, &model, launched, launched));
    peek_landed(&mut model, &mut echo, "/");
    path_listed(&mut hold, &mut model, &mut echo, "/p", 2, launched);
    assert!(model.peek_out());
    rows_landed(&mut model, &mut echo);
    assert!(!paint_with(&hold, &model, launched, launched));
    peek_landed(&mut model, &mut echo, "/");
    assert!(paint_with(&hold, &model, launched, launched));
    crate::tui::wire::finish(echo, handle);
}
// A failed parent peek still answers, so the hold ends on the reply and never waits out the cap.
#[test]
fn failed_parent_peek_releases_the_hold() {
    let mut hold = Hold::new();
    hold.mark_painted();
    let launched = Instant::now();
    let (mut echo, handle) = crate::tui::wire::echo_wire();
    let mut model = Model::new(PathBuf::from("/p"), &jsondoc::Json::Null);
    path_listed(&mut hold, &mut model, &mut echo, "/p", 2, launched);
    assert!(model.peek_out());
    rows_landed(&mut model, &mut echo);
    assert!(!paint_with(&hold, &model, launched, launched));
    failed_peek_landed(&mut model, &mut echo, "/");
    assert!(!model.peek_out());
    assert!(paint_with(&hold, &model, launched, launched));
    crate::tui::wire::finish(echo, handle);
}
// A peeked that lands before its rows draws nothing: the rows map is still empty, so the hold stays until the rows land.
#[test]
fn peeked_before_rows_draws_nothing() {
    let mut hold = Hold::new();
    hold.mark_painted();
    let launched = Instant::now();
    let (mut echo, handle) = crate::tui::wire::echo_wire();
    let mut model = Model::new(PathBuf::from("/fx"), &jsondoc::Json::Null);
    path_listed(&mut hold, &mut model, &mut echo, "/fx/child", 4, launched);
    assert!(model.peek_out());
    peek_landed(&mut model, &mut echo, "/fx");
    assert!(model.rows.is_empty());
    assert!(!paint_with(&hold, &model, launched, launched));
    crate::tui::wire::finish(echo, handle);
}
// The wait budget never passes the hold deadline: the deadline remainder across the hold window, then the idle quantum.
#[test]
fn wait_budget_never_passes_the_deadline() {
    let mut hold = Hold::new();
    let launched = Instant::now();
    let (model, wire, reader) = listed_model(&mut hold, 5, launched);
    for ms in [0, 1, 50, 99, 100, 110, 149, 150, 151, 300] {
        let now = launched + Duration::from_millis(ms);
        let remain = 150 - ms as i32;
        let want = if remain > 0 { remain.min(100) } else { 100 };
        assert_eq!(hold.wait_ms(&model, now, launched, 100), want, "at +{} ms", ms);
    }
    crate::tui::wire::finish(wire, reader);
}
// The hold only shortens the loop's input wait: a waiting Escape's shorter remainder and a long idle wait both pass through.
#[test]
fn wait_budget_never_lengthens_the_input_wait() {
    let mut hold = Hold::new();
    let launched = Instant::now();
    let (mut model, mut wire, reader) = listed_model(&mut hold, 5, launched);
    assert_eq!(hold.wait_ms(&model, launched + Duration::from_millis(110), launched, 3), 3);
    assert_eq!(hold.wait_ms(&model, launched + Duration::from_millis(110), launched, 5000), 40);
    assert_eq!(hold.wait_ms(&model, launched + Duration::from_millis(300), launched, 5000), 5000);
    rows_landed(&mut model, &mut wire);
    peek_landed(&mut model, &mut wire, "/");
    assert_eq!(hold.wait_ms(&model, launched + Duration::from_millis(110), launched, 5000), 5000);
    crate::tui::wire::finish(wire, reader);
}
// A deadline under a millisecond away still waits one millisecond, so the loop never spins on zero waits up to it.
#[test]
fn wait_budget_rounds_a_partial_millisecond_up() {
    let mut hold = Hold::new();
    let launched = Instant::now();
    let (model, wire, reader) = listed_model(&mut hold, 5, launched);
    assert_eq!(hold.wait_ms(&model, launched + Duration::from_micros(149_600), launched, 100), 1, "a partial millisecond waited nothing");
    assert_eq!(hold.wait_ms(&model, launched + Duration::from_micros(109_600), launched, 100), 41);
    crate::tui::wire::finish(wire, reader);
}
