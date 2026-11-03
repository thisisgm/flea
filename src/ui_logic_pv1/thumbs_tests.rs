// Ports the value checks of tests/js/thumbs.js to thumbs.rs. Each comment above an assert is the JS check label.
use super::thumbs::*;

fn row(name: &str, thumbnailable: bool) -> Row {
    Row { name: name.into(), thumbnailable, ..Default::default() }
}

fn image(thumbnailable: bool) -> Row {
    Row { thumbnailable, icon: "image-x-generic".into(), ..Default::default() }
}

fn video(thumbnailable: bool) -> Row {
    Row { thumbnailable, icon: "video-x-generic".into(), ..Default::default() }
}

fn ids(v: &[usize]) -> String {
    v.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")
}

// A held window starting at 10, four rows, the third of which no thumbnailer declares.
fn held_rows() -> Vec<Row> {
    vec![row("a", true), row("b", true), row("c", false), row("d", true)]
}

// JS Math.round for the non-negative sizes these checks use.
fn drawn(scale: f64, w: f64, h: f64) -> String {
    format!("{}x{}", (w * scale + 0.5).floor(), (h * scale + 0.5).floor())
}

fn asked(ask: &[usize]) -> Plan {
    Plan { ask: ask.to_vec(), drop: Vec::new(), cache_only: false }
}

fn cache_asked(ask: &[usize]) -> Plan {
    Plan { ask: ask.to_vec(), drop: Vec::new(), cache_only: true }
}

// A state built from entries, in the order given, the way the JS checks build {file, order}.
fn map_of(entries: &[(usize, Entry)]) -> Map {
    let mut m = Map::default();
    for (k, e) in entries {
        m.file.insert(*k, e.clone());
        m.order.push(*k);
    }
    m
}

#[test]
fn policy_modes_gate_thumbnail_sources() {
    // off skips every thumbnail source
    assert!(!allowed(Some(&image(true)), Mode::Off));
    // images excludes video sources
    assert!(!allowed(Some(&video(true)), Mode::Images));
    // images retains image sources
    assert!(allowed(Some(&image(true)), Mode::Images));
    // media retains eligible video sources
    assert!(allowed(Some(&video(true)), Mode::Media));
}

#[test]
fn switching_policy_cancels_only_what_it_must() {
    let policy_rows = vec![image(true), video(true), video(true)];
    let mut pending = Map::default();
    pending.applied(&asked(&[400, 401, 402]));
    pending.remember(402, "/cache/402.png", 240);
    let images_only = pending.plan(&policy_rows, 400, 400, 402, Mode::Images);
    // switching to images cancels pending visible video without cancelling images or completed work
    assert_eq!(ids(&images_only.drop), "401");
    pending.applied(&images_only);
    let disabled = pending.plan(&policy_rows, 400, 400, 402, Mode::Off);
    // switching off cancels the remaining pending visible image
    assert_eq!(ids(&disabled.drop), "400");
    // switching off never starts replacement work
    assert_eq!(disabled.ask.len(), 0);
    pending.applied(&disabled);
    // policy changes preserve completed cached thumbnails
    assert_eq!(pending.file_for(402), "/cache/402.png");
    // reenabling media requests cancelled work without regenerating completed work
    assert_eq!(ids(&pending.plan(&policy_rows, 400, 400, 402, Mode::Media).ask), "400,401");
}

#[test]
fn viewport_asks_each_visible_thumbnailable_row_once() {
    let rows = held_rows();
    let mut s = Map::default();
    let work = s.plan(&rows, 10, 10, 13, Mode::Media);
    // only thumbnailable rows are asked for
    assert_eq!(ids(&work.ask), "10,11,13");
    // nothing is cancelled on a first pass
    assert_eq!(work.drop.len(), 0);
    s.applied(&work);
    // a row already asked is not asked twice
    assert_eq!(s.plan(&rows, 10, 10, 13, Mode::Media).ask.len(), 0);
    // a row asked and still waiting reports no file
    assert_eq!(s.file_for(10), "");
    let work = s.plan(&rows, 10, 12, 13, Mode::Media);
    // a row the viewport left is cancelled
    assert_eq!(ids(&work.drop), "10,11");
    // and a row still visible is not
    assert_eq!(work.ask.len(), 0);
    s.applied(&work);
    // a cancelled row leaves the order too
    assert_eq!(s.order.len(), 1);
    // a cancelled row can be asked for again
    assert_eq!(ids(&s.plan(&rows, 10, 10, 13, Mode::Media).ask), "10,11");
    s.remember(13, "/cache/13.png", 240);
    // an answered row reports its file
    assert_eq!(s.file_for(13), "/cache/13.png");
    // and is never asked for again
    assert_eq!(s.plan(&rows, 10, 13, 13, Mode::Media).ask.len(), 0);
    // and is never cancelled, having no job to drop
    assert!(!s.plan(&rows, 10, 0, 1, Mode::Media).drop.contains(&13));
    let mut cache = Map::default();
    cache.applied(&cache_asked(&[10, 11]));
    // a cache-only ask the viewport left is cancelled too
    assert_eq!(ids(&cache.plan(&rows, 10, 12, 13, Mode::Media).drop), "10,11");
    // and a cache-only ask counts as pending
    assert!(cache.file[&10].pending());
    s.remember(11, "", 240);
    // a row answered with no thumbnail reports nothing
    assert_eq!(s.file_for(11), "");
    // and is never asked again either
    assert_eq!(s.plan(&rows, 10, 11, 11, Mode::Media).ask.len(), 0);
    // an unknown row reports nothing
    assert_eq!(Map::default().file_for(4), "");
    // a row outside the held window is not asked for
    assert_eq!(Map::default().plan(&rows, 10, 0, 3, Mode::Media).ask.len(), 0);
    // a row above the viewport is not asked for
    assert_eq!(ids(&Map::default().plan(&rows, 0, 2, 3, Mode::Media).ask), "3");
}

#[test]
fn bounded_order_evicts_oldest() {
    let mut capped = Map::default();
    for i in 0..5 {
        capped.remember(i, &format!("/cache/{i}.png"), 3);
    }
    // the cap evicts the oldest entry
    assert_eq!(capped.file_for(0), "");
    // and keeps the newest
    assert_eq!(capped.file_for(4), "/cache/4.png");
    // and holds exactly the cap
    assert_eq!(capped.order.len(), 3);
    capped.remember(4, "/cache/4b.png", 3);
    // answering one row twice does not grow the order
    assert_eq!(capped.order.len(), 3);
    // and the newer answer wins
    assert_eq!(capped.file_for(4), "/cache/4b.png");
}

#[test]
fn empty_states_are_independent() {
    let mut a = Map::default();
    let b = Map::default();
    a.remember(1, "/x.png", 240);
    // each empty state is its own: JS checks identity, so here a write to one leaves the other empty
    assert_eq!(b, Map::default());
}

#[test]
fn refusal_is_an_empty_answer_not_a_wait() {
    let rows = held_rows();
    let mut r = Map::default();
    let work = r.plan(&rows, 10, 10, 11, Mode::Media);
    r.applied(&work);
    // a row still waiting is not a refusal
    assert!(!r.refused(10));
    // a row never asked about is not a refusal
    assert!(!r.refused(99));
    r.remember(10, "", 8);
    // an empty answer is a refusal
    assert!(r.refused(10));
    // and it still reads as no file
    assert_eq!(r.file_for(10), "");
    r.remember(11, "/cache/b.png", 8);
    // a real answer is not a refusal
    assert!(!r.refused(11));
}

#[test]
fn cache_only_mark_reads_as_not_tried() {
    let mut c = Map::default();
    c.applied(&cache_asked(&[7]));
    // a cache-only ask marks the row without naming a file
    assert_eq!(c.file_for(7), "");
    // and the marked row is not a refusal
    assert!(!c.refused(7));
    // and is not asked twice while marked
    assert_eq!(c.plan(&[row("", true)], 7, 7, 7, Mode::Media).ask.len(), 0);
    c.remember(7, "", 240);
    // its empty answer still reads as no file
    assert_eq!(c.file_for(7), "");
    // but still not as a refusal
    assert!(!c.refused(7));
    c.remember(7, "/cache/7.png", 240);
    // a real answer overwrites the mark
    assert_eq!(c.file_for(7), "/cache/7.png");
}

#[test]
fn viewport_clamps_to_listing() {
    let v = viewport(0.0, 37.0, 36.0, 2000);
    // a viewport at the top starts at row zero
    assert_eq!((v.first, v.last), (0, 35));
    let v = viewport(370.0, 37.0, 36.0, 2000);
    // a scrolled viewport starts at the row under contentY
    assert_eq!((v.first, v.last), (10, 45));
    let v = viewport(0.0, 37.0, 36.0, 4);
    // a listing shorter than the screen clamps to its last row
    assert_eq!((v.first, v.last), (0, 3));
    let v = viewport(18.0, 37.0, 36.0, 2000);
    // a part-scrolled row counts as the first visible one
    assert_eq!(v.first, 0);
    let v = viewport(21.0, 37.0, 36.0, 1000);
    // a part-scrolled window reaches the row its bottom edge straddles
    assert_eq!((v.first, v.last), (0, 36));
    let v = viewport(21.0, 37.0, 36.0, 30);
    // and that straddled row is still clamped to the last row of the listing
    assert_eq!(v.last, 29);
}

#[test]
fn small_originals_draw_at_their_own_size() {
    // a 120x68 original in the 754x471 frame draws at its own size
    assert_eq!(drawn(fit_scale(754.0, 471.0, 120.0, 68.0, 1.0), 120.0, 68.0), "120x68");
    // an original decoded to fit is drawn as decoded
    assert_eq!(drawn(fit_scale(754.0, 471.0, 707.0, 471.0, 1.0), 707.0, 471.0), "707x471");
    // so is one that fits the width
    assert_eq!(drawn(fit_scale(2099.0, 1156.0, 2099.0, 700.0, 1.0), 2099.0, 700.0), "2099x700");
    // a full-size cache file of a photo whose size is not known yet fills the frame
    assert_eq!(drawn(fit_scale(754.0, 471.0, 256.0, 171.0, thumb_limit(256.0, 171.0, 0.0, 0.0)), 256.0, 171.0), "705x471");
    // of a 6000x4000 photo too
    assert_eq!(drawn(fit_scale(754.0, 471.0, 256.0, 171.0, thumb_limit(256.0, 171.0, 6000.0, 4000.0)), 256.0, 171.0), "705x471");
    // of a 300x200 picture only up to its own size
    assert_eq!(drawn(fit_scale(754.0, 471.0, 256.0, 171.0, thumb_limit(256.0, 171.0, 300.0, 200.0)), 256.0, 171.0), "300x200");
    // and an EXIF-turned one by its longest side
    assert_eq!(drawn(fit_scale(754.0, 471.0, 171.0, 256.0, thumb_limit(171.0, 256.0, 300.0, 200.0)), 171.0, 256.0), "200x300");
    // a cache file smaller than the cache size is the original's own size, never enlarged
    assert_eq!(drawn(fit_scale(754.0, 471.0, 64.0, 48.0, thumb_limit(64.0, 48.0, 0.0, 0.0)), 64.0, 48.0), "64x48");
    // a frame smaller than the picture still shrinks it
    assert_eq!(drawn(fit_scale(200.0, 125.0, 256.0, 171.0, thumb_limit(256.0, 171.0, 6000.0, 4000.0)), 256.0, 171.0), "187x125");
}

#[test]
fn vector_preview_fills_and_raster_preview_caps() {
    // EY-64: a 60x40 vector in the 274x171 Columns frame fills along the limiting axis.
    assert_eq!(preview_limit(false, true, 60.0, 40.0, 60.0, 40.0), f64::INFINITY);
    assert_eq!(drawn(fit_scale(274.0, 171.0, 60.0, 40.0, preview_limit(false, true, 60.0, 40.0, 60.0, 40.0)), 60.0, 40.0), "257x171");
    // A 60x40 raster keeps the cap: drawn at its own pixels.
    assert_eq!(drawn(fit_scale(274.0, 171.0, 60.0, 40.0, preview_limit(false, false, 60.0, 40.0, 60.0, 40.0)), 60.0, 40.0), "60x40");
}

#[test]
fn video_poster_fills_and_image_poster_caps() {
    // a video poster is uncapped
    assert_eq!(preview_limit(true, false, 256.0, 256.0, 64.0, 64.0), f64::INFINITY);
    // an image poster keeps its cap
    assert_eq!(preview_limit(false, false, 256.0, 256.0, 64.0, 64.0), 64.0 / 256.0);
    // a 64x64 clip poster in the 754x471 frame fills it
    assert_eq!(drawn(fit_scale(754.0, 471.0, 256.0, 256.0, preview_limit(true, false, 256.0, 256.0, 64.0, 64.0)), 256.0, 256.0), "471x471");
}

#[test]
fn prefetch_miss_on_generating_class_returns_to_unasked() {
    let mut generating = Map::default();
    generating.applied(&cache_asked(&[3]));
    generating.miss(3, true, 240);
    // a prefetch miss on a generating class leaves the row undefined
    assert_eq!(generating.file.get(&3), None);
    // and drops it from the order
    assert_eq!(ids(&generating.order), "");
    let mut cache_class = Map::default();
    cache_class.applied(&cache_asked(&[3]));
    cache_class.miss(3, false, 240);
    // on a cache-only class it stays a miss
    assert_eq!(cache_class.file.get(&3), Some(&Entry::CacheMissed));
}

#[test]
fn planner_asks_a_missed_row_once_then_never_again() {
    let rows: Vec<Row> = (0..5).map(|i| Row { name: format!("f{i}"), ..image(true) }).collect();
    let planned = |entries: &[(usize, Entry)]| ids(&map_of(entries).plan(&rows, 0, 0, 4, Mode::Media).ask);
    let mut answered = vec![
        (0, Entry::File("/c/0.png".into())),
        (1, Entry::File("/c/1.png".into())),
        (2, Entry::File("/c/2.png".into())),
        (4, Entry::File("/c/4.png".into())),
    ];
    // the planner asks a row a prefetch miss left unasked
    assert_eq!(planned(&answered), "3");
    answered.push((3, Entry::CacheMissed));
    // and never re-asks a row that stays missed
    assert_eq!(planned(&answered), "");
}

#[test]
fn carried_answers_follow_their_names() {
    let mut live = Map::default();
    live.applied(&asked(&[10, 11, 12, 13]));
    live.remember(10, "/cache/a.png", 240);
    live.remember(12, "/cache/c.png", 240);
    live.remember(13, "", 240);
    let old_rows = vec![row("a", true), row("b", true), row("c", true), row("d", true)];
    let new_rows = vec![row("x", true), row("c", true), row("a", false), row("b", true), row("d", true)];
    let moved = live.carried(&old_rows, 10, &new_rows, 20);
    // a finished thumbnail follows its name to its new index
    assert_eq!(moved.file_for(21), "/cache/c.png");
    // a row no longer thumbnailable carries nothing
    assert_eq!(moved.file_for(22), "");
    // an asked row is not carried and is asked again
    assert_eq!(ids(&moved.plan(&[row("b", true)], 23, 23, 23, Mode::Media).ask), "23");
    // an answer of nothing is not carried either
    assert!(!moved.refused(24));
    // a row new to the answer is asked for
    assert_eq!(ids(&moved.plan(&[row("x", true)], 20, 20, 20, Mode::Media).ask), "20");
    // a carried row is answered, so never asked for
    assert_eq!(moved.plan(&[row("c", true)], 21, 21, 21, Mode::Media).ask.len(), 0);
    // the carried order holds only the carried rows
    assert_eq!(ids(&moved.order), "21");
}

#[test]
fn cache_asks_and_misses_are_not_carried() {
    let old_rows = vec![row("a", true), row("b", true), row("c", true), row("d", true)];
    let marks = map_of(&[(10, Entry::CacheAsked), (11, Entry::CacheMissed)]);
    // a cache-only ask and a cache miss are not carried
    assert_eq!(marks.carried(&old_rows, 10, &old_rows, 10).order.len(), 0);
    let live = map_of(&[(10, Entry::File("/cache/a.png".into())), (11, Entry::Asked), (12, Entry::File("/cache/c.png".into())), (13, Entry::File(String::new()))]);
    // a name like a prototype key carries nothing it was not given
    assert_eq!(live.carried(&[row("a", true)], 10, &[row("constructor", true)], 0).order.len(), 0);
}
