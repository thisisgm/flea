// propertyfacts.js PropertyFacts checks for property.rs; the three styled checks stay out, StyledText has no GPUI form.
use super::photo::Photo;
use super::property::*;

fn drawn(line: &Line) -> String {
    format!("{}{}", line.value, line.tail)
}

fn pair(key: &str, value: &str) -> (String, String) {
    (key.to_string(), value.to_string())
}

fn labels(pairs: &[(String, String)]) -> String {
    pairs.iter().map(|p| p.0.clone()).collect::<Vec<_>>().join(",")
}

#[test]
fn walk_while_counting_and_done() {
    let i18n = super::english();
    let counting = Walk { bytes: 2_104_332_800, items: 812, partial: false, done: false };
    // Size while counting is the floor with the tail
    assert_eq!(drawn(&size(0, Some(&counting), &i18n)), "> 2.1 GB (2104332800 bytes) \u{b7} counting\u{2026}");
    // Contains while counting is the floor with the tail
    assert_eq!(drawn(&contains(Some(&counting), &i18n).unwrap()), "> 812 items \u{b7} counting\u{2026}");
    // the tail is its own run, so it can wear the muted colour
    assert_eq!(size(0, Some(&counting), &i18n).tail, " \u{b7} counting\u{2026}");

    let done = Walk { bytes: 7_404_118_016, items: 1204, partial: false, done: true };
    // Size done is exact with no prefix and no tail
    assert_eq!(drawn(&size(0, Some(&done), &i18n)), "7.4 GB (7404118016 bytes)");
    // Contains done is grouped
    assert_eq!(drawn(&contains(Some(&done), &i18n).unwrap()), "1,204 items");
}

#[test]
fn first_frame_and_single_entry_counts() {
    let i18n = super::english();
    let first = begin(4096);
    // the first frame counts the entry's own size
    assert_eq!(drawn(&size(4096, Some(&first), &i18n)), "> 4.1 kB (4096 bytes) \u{b7} counting\u{2026}");
    // and nothing below it
    assert_eq!(drawn(&contains(Some(&first), &i18n).unwrap()), "> 0 items \u{b7} counting\u{2026}");
    // one entry reads item, not items
    let one_done = Walk { bytes: 4096, items: 1, partial: false, done: true };
    assert_eq!(drawn(&contains(Some(&one_done), &i18n).unwrap()), "1 item");
    // one entry while counting is still a floor
    let one_counting = Walk { bytes: 4096, items: 1, partial: false, done: false };
    assert_eq!(drawn(&contains(Some(&one_counting), &i18n).unwrap()), "> 1 item \u{b7} counting\u{2026}");
    // an empty folder done reads 0 items
    let empty = Walk { bytes: 4096, items: 0, partial: false, done: true };
    assert_eq!(drawn(&contains(Some(&empty), &i18n).unwrap()), "0 items");
}

#[test]
fn partial_done_walk_stays_a_floor_without_tail() {
    let i18n = super::english();
    let cut = Walk { bytes: 5000, items: 3, partial: true, done: true };
    // a partial done Size stays a floor
    assert_eq!(drawn(&size(0, Some(&cut), &i18n)), "> 5.0 kB (5000 bytes)");
    // a partial done Contains stays a floor
    assert_eq!(drawn(&contains(Some(&cut), &i18n).unwrap()), "> 3 items");
    // a partial done floor has no tail
    let tails = format!("{}{}", size(0, Some(&cut), &i18n).tail, contains(Some(&cut), &i18n).unwrap().tail);
    assert_eq!(tails, "");
}

#[test]
fn file_has_no_walk_and_no_contains() {
    let i18n = super::english();
    // a file's Size is unchanged
    assert_eq!(drawn(&size(13, None, &i18n)), "13 B (13 bytes)");
    // a file's Size has no tail
    assert_eq!(size(13, None, &i18n).tail, "");
    // a file has no Contains
    assert_eq!(contains(None, &i18n), None);
}

#[test]
fn replies_fold_into_the_walk() {
    let first = begin(4096);
    let progress_reply = Reply { ok: true, bytes: 9000, items: 5, ..Default::default() };
    let progress = next(first, &progress_reply);
    // a progress reply moves both numbers and keeps counting
    assert_eq!(progress, Walk { bytes: 9000, items: 5, partial: false, done: false });
    let last = next(progress, &Reply { ok: true, bytes: 9500, items: 6, done: true, ..Default::default() });
    // the final reply ends the count
    assert!(last.done);
    let cancelled = Reply { ok: true, cancelled: true, bytes: 1, items: 1, ..Default::default() };
    // a cancelled walk settles as the floor already counted
    assert_eq!(next(progress, &cancelled), Walk { bytes: 9000, items: 5, partial: true, done: true });
    // a cancelled reply after done changes nothing
    assert_eq!(next(last, &cancelled), last);
    let refused = Reply { ok: false, ..Default::default() };
    // a refused request ends as the floor already counted
    assert_eq!(next(progress, &refused), Walk { bytes: 9000, items: 5, partial: true, done: true });
}

#[test]
fn extra_pairs_for_a_video() {
    let i18n = super::english();
    let video = Facts { ok: true, vcodec: "h264".into(), acodec: "aac".into(), bps: 8_100_000.0, ..Default::default() };
    // a video gets Codecs then Bit rate
    assert_eq!(extra_pairs(Some(&video), None, &i18n), vec![pair("Codecs", "H.264 \u{b7} AAC"), pair("Bit rate", "8.1 Mb/s")]);
    let no_rate = Facts { ok: true, vcodec: "h264".into(), acodec: String::new(), bps: 0.0, ..Default::default() };
    // a video with no bit rate gets Codecs alone
    assert_eq!(labels(&extra_pairs(Some(&no_rate), None, &i18n)), "Codecs");
}

#[test]
fn extra_pairs_for_a_photo() {
    let i18n = super::english();
    let photo = Facts {
        ok: true,
        make: "Canon".into(),
        model: "Canon EOS R6".into(),
        lens: "RF35mm F1.8 MACRO IS STM".into(),
        shutter: Some((1.0, 250.0)),
        fnumber: Some((28.0, 10.0)),
        iso: 400,
        taken: "2026:09:12 18:31:07".into(),
        ..Default::default()
    };
    // a photo gets Camera, Lens, Exposure, Taken in the board's order
    assert_eq!(
        extra_pairs(Some(&photo), None, &i18n),
        vec![
            pair("Camera", "Canon EOS R6"),
            pair("Lens", "RF35mm F1.8 MACRO IS STM"),
            pair("Exposure", "1/250 s \u{b7} f/2.8 \u{b7} ISO 400"),
            pair("Taken", "2026-09-12 18:31"),
        ]
    );
    let no_lens = Facts {
        ok: true,
        make: "FUJIFILM".into(),
        model: "X100V".into(),
        shutter: Some((1.0, 500.0)),
        fnumber: Some((56.0, 10.0)),
        iso: 160,
        taken: "2026:01:02 03:04:05".into(),
        ..Default::default()
    };
    // a photo with no lens leaves no empty pair
    assert_eq!(labels(&extra_pairs(Some(&no_lens), None, &i18n)), "Camera,Exposure,Taken");
}

#[test]
fn extra_pairs_adds_nothing_without_facts() {
    let i18n = super::english();
    let bare = Facts { ok: true, ..Default::default() };
    // a file with no facts adds nothing
    assert_eq!(labels(&extra_pairs(Some(&bare), None, &i18n)), "");
    assert_eq!(labels(&extra_pairs(None, None, &i18n)), "");
    assert_eq!(labels(&extra_pairs(Some(&Facts::default()), None, &i18n)), "");
    let refused = Facts { ok: false, ..Default::default() };
    // a refused facts reply adds nothing
    assert_eq!(labels(&extra_pairs(Some(&refused), None, &i18n)), "");
}

#[test]
fn extra_pairs_merges_immediate_photo_with_deferred_facts() {
    let i18n = super::english();
    let immediate = Photo {
        make: "Canon".into(),
        model: "Canon EOS R6".into(),
        lens: "RF35mm F1.8 MACRO IS STM".into(),
        exp: Some((1.0, 2.0)),
        fnum: Some((4.0, 1.0)),
        iso: 400,
        taken: "2026:09:12 18:31:07".into(),
        ..Default::default()
    };
    let absent = Facts {
        ok: true,
        shutter: Some((0.0, 0.0)),
        fnumber: Some((1.0, 0.0)),
        ..Default::default()
    };
    // absent deferred fields preserve immediate photo values and exposure language
    assert_eq!(
        extra_pairs(Some(&absent), Some(&immediate), &i18n),
        vec![
            pair("Camera", "Canon EOS R6"),
            pair("Lens", "RF35mm F1.8 MACRO IS STM"),
            pair("Exposure", "0.5 s \u{b7} f/4 \u{b7} ISO 400"),
            pair("Taken", "2026-09-12 18:31"),
        ]
    );
    let deferred = Facts { ok: true, shutter: Some((1.0, 2.0)), ..Default::default() };
    // deferred-only photos retain their existing exposure language
    assert_eq!(extra_pairs(Some(&deferred), None, &i18n), vec![pair("Exposure", "1/2 s")]);
}
