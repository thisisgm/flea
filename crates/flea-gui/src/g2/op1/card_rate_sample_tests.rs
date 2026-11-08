//! One beat's sample draws the whole card, and an extraction of many tiny members keeps a time left it can meet.
use super::*;

fn extracting(done: u64, bytes: u64) -> JobRow {
    JobRow {
        id: 1,
        kind: "extract".into(),
        head: format!("Extracting {} of 30002", done + 1),
        file: format!("f{done}.txt"),
        done,
        total: 30_002,
        bytes: Some(bytes),
        total_bytes: Some(400_200_000),
        item_bytes: 7,
        state: "running".into(),
        ..Default::default()
    }
}

fn publish(rates: &mut Rates, source: &JobRow, now: u64) -> JobRow {
    let mut rows = [source.clone()];
    rates.publish(&mut rows, now);
    rows[0].clone()
}

#[test]
fn card_rate_draws_the_head_file_and_bytes_from_one_beat() {
    let catalog = crate::test_catalogs::english();
    let mut rates = Rates::default();
    publish(&mut rates, &extracting(1, 3), 0);
    let beat = extracting(1128, 7_900);
    publish(&mut rates, &beat, 250);
    // Between beats the wire runs ahead; the card keeps the beat's count beside the beat's bytes.
    let drawn = publish(&mut rates, &extracting(10_771, 64_291), 400);
    assert_eq!(
        (&drawn.head, &drawn.file, drawn.done, drawn.bytes),
        (&beat.head, &beat.file, 1128, Some(7_900))
    );
    let block = jobs::block(&catalog, &drawn);
    assert_eq!(block.head, "Extracting 1129 of 30002");
    assert!(block.file.starts_with("f1128.txt"), "{}", block.file);
    let drawn = publish(&mut rates, &extracting(10_771, 64_291), 500);
    assert_eq!((drawn.done, drawn.bytes), (10_771, Some(64_291)));
    assert_eq!(drawn.head, "Extracting 10772 of 30002");
}

#[test]
fn card_rate_extract_names_no_time_left_while_member_and_byte_paces_disagree() {
    let catalog = crate::test_catalogs::english();
    let mut rates = Rates::default();
    publish(&mut rates, &extracting(1, 3), 0);
    let sample = publish(&mut rates, &extracting(1128, 7_900), 250);
    // 1,127 members in 250 ms leave 6 s, 31.6 kB/s leaves 3:31:09: no honest figure lies between them.
    assert_eq!((sample.rate, sample.left), (31_588, None));
    assert_eq!(
        flea::ui_logic_op1::joined(&jobs::block(&catalog, &sample).line),
        "7.9 kB of 400.2 MB · 31.6 kB/s"
    );
    // The large member is in flight while the window still holds the tiny members: 0 s against 12 s.
    let sample = publish(&mut rates, &extracting(1128, 300_007_900), 500);
    assert_eq!(sample.left, None);
    // No member finished in the window, and a 400 MB member cannot time the 28,874 still to come.
    let large = JobRow {
        item_bytes: 400_000_000,
        ..extracting(1128, 400_000_000)
    };
    assert_eq!(publish(&mut rates, &large, 2_500).left, None);
}

fn members(done: u64, total: u64, bytes: u64, total_bytes: u64) -> JobRow {
    JobRow {
        done,
        total,
        bytes: Some(bytes),
        total_bytes: Some(total_bytes),
        item_bytes: total_bytes / total,
        ..extracting(done, bytes)
    }
}

#[test]
fn card_rate_extract_paces_that_agree_name_the_slower_one() {
    let mut rates = Rates::default();
    publish(&mut rates, &members(1, 1000, 1_000_000, 1_000_000_000), 0);
    // Bytes leave 8 s at 120 MB/s, 25 members in 250 ms leave 9 s.
    let sample = publish(
        &mut rates,
        &members(26, 1000, 31_000_000, 1_000_000_000),
        250,
    );
    assert_eq!((sample.rate, sample.left), (120_000_000, Some(9)));
    let mut rates = Rates::default();
    publish(&mut rates, &members(1, 1000, 1_000_000, 1_000_000_000), 0);
    // Bytes leave 9 s at 100 MB/s, 30 members in 250 ms leave 8 s.
    let sample = publish(
        &mut rates,
        &members(31, 1000, 26_000_000, 1_000_000_000),
        250,
    );
    assert_eq!((sample.rate, sample.left), (100_000_000, Some(9)));
}

#[test]
fn card_rate_one_member_finished_in_the_window_leaves_the_byte_pace_alone() {
    let mut rates = Rates::default();
    // Five members of 2 GB at 200 MB/s: the first finishes at 10 s, after starting long before the window.
    for now in (0..10_000).step_by(250) {
        publish(
            &mut rates,
            &members(0, 5, now * 200_000, 10_000_000_000),
            now,
        );
    }
    let sample = publish(
        &mut rates,
        &members(1, 5, 2_000_000_000, 10_000_000_000),
        10_000,
    );
    // One member in 2.25 s would say 9 s; the bytes say 40 s and are the only reading.
    assert_eq!((sample.rate, sample.left), (200_000_000, Some(40)));
    let sample = publish(
        &mut rates,
        &members(1, 5, 2_050_000_000, 10_000_000_000),
        10_250,
    );
    assert_eq!(sample.left, Some(39));
}

#[test]
fn card_rate_member_pace_stops_speaking_once_the_window_holds_no_finished_member() {
    let mut rates = Rates::default();
    publish(&mut rates, &extracting(1, 3), 0);
    assert_eq!(
        publish(&mut rates, &extracting(20_000, 140_000), 250).left,
        None
    );
    publish(&mut rates, &extracting(20_000, 10_140_000), 500);
    // The window starts at 250 ms with no member finished since: 20 MB/s leaves 17 s.
    let large = JobRow {
        item_bytes: 380_000_000,
        ..extracting(20_000, 45_140_000)
    };
    let sample = publish(&mut rates, &large, 2_500);
    assert_eq!((sample.rate, sample.left), (20_000_000, Some(17)));
    // A tiny member in flight cannot speak for 350 MB still to come.
    let sample = publish(&mut rates, &extracting(20_000, 50_140_000), 2_750);
    assert_eq!(sample.left, None);
}

#[test]
fn card_rate_paces_agree_within_their_factor_and_a_sub_second_pace_counts_as_one() {
    assert_eq!(agreed(8, 4), Some(8));
    assert_eq!(agreed(4, 9), None);
    assert_eq!(agreed(0, 2), Some(2));
    assert_eq!(agreed(3, 0), None);
    assert_eq!(agreed(0, 0), Some(0));
}

#[test]
fn card_rate_member_pace_never_names_a_time_left_for_stalled_bytes_or_other_kinds() {
    let mut rates = Rates::default();
    publish(&mut rates, &extracting(1, 3), 0);
    let sample = publish(&mut rates, &extracting(500, 3), 250);
    assert_eq!((sample.rate, sample.left), (0, None));
    for kind in ["copy", "move"] {
        let mut rates = Rates::default();
        let mut input = extracting(1, 3);
        input.kind = kind.into();
        publish(&mut rates, &input, 0);
        input.done = 1128;
        input.bytes = Some(7_900);
        let sample = publish(&mut rates, &input, 250);
        assert_eq!(sample.left, Some(400_192_100 / 31_588), "{kind}");
    }
}

#[test]
fn card_rate_slow_small_members_never_reach_the_byte_pace_alone() {
    let mut rates = Rates::default();
    // 1,000 members of 10 kB, one every 400 ms, then a 390 MB member: some beats see no member finish.
    for now in (0..=6_000).step_by(250) {
        let done = now / 400;
        let sample = publish(
            &mut rates,
            &JobRow {
                item_bytes: 10_000,
                ..members(done, 1001, done * 10_000, 400_000_000)
            },
            now,
        );
        if now >= 1_000 {
            assert_eq!(sample.left, None, "at {now} ms");
        }
    }
}

#[test]
fn card_rate_one_finish_per_window_never_reaches_the_byte_pace_alone() {
    let mut rates = Rates::default();
    // 1,000 members of 10 kB, one every 1,200 ms, then a 390 MB member: windows hold one finish or two.
    for now in (0..=12_000).step_by(250) {
        let done = now / 1_200;
        let sample = publish(
            &mut rates,
            &JobRow {
                item_bytes: 10_000,
                ..members(done, 1001, done * 10_000, 400_000_000)
            },
            now,
        );
        if now >= 2_500 {
            assert_eq!(sample.left, None, "at {now} ms");
        }
    }
}

#[test]
fn card_rate_a_large_member_never_times_the_small_members_after_it() {
    let mut rates = Rates::default();
    // A 400 MB member at 200 MB/s, then 30,000 members of 10 kB: the bytes alone say 3 s and leave out every member's cost.
    for now in (0..=1_750).step_by(250) {
        let sample = publish(
            &mut rates,
            &JobRow {
                item_bytes: 400_000_000,
                ..members(0, 30_001, now * 200_000, 700_000_000)
            },
            now,
        );
        if now > 0 {
            assert_eq!(sample.left, None, "at {now} ms");
        }
    }
}

#[test]
fn card_rate_bytes_speak_only_within_the_member_size_and_member_cost_bounds() {
    // The member in flight against an average remaining one, 2x apart and then past it.
    assert!(bytes_speak(4, 1, 8, 0));
    assert!(!bytes_speak(4, 1, 9, 0));
    // 1,000 members after it cost 1 s, as much as 1 MB at 1 MB/s, and one more tips it.
    assert!(bytes_speak(1_000_000, 1_001, 1_000_000, 1_000_000));
    assert!(!bytes_speak(1_000_000, 1_002, 1_000_000, 1_000_000));
    // The last member's own cost is paid however far through it the job is.
    assert!(bytes_speak(999_000_000, 1, 499_000_000, 200_000_000));
    // A row with no member counts keeps its bytes.
    assert!(bytes_speak(0, 0, 60_000_000, 1));
}

#[test]
fn card_rate_a_large_member_keeps_its_time_left_before_a_small_one() {
    let mut rates = Rates::default();
    // A 400 MB member at 200 MB/s, then one of 1 kB: one member's cost cannot move the bytes' figure.
    for now in (0..=1_750).step_by(250) {
        let sample = publish(
            &mut rates,
            &JobRow {
                item_bytes: 400_000_000,
                ..members(0, 2, now * 200_000, 400_001_000)
            },
            now,
        );
        if now > 0 {
            let left = (400_001_000 - now * 200_000) / 200_000_000;
            assert_eq!(sample.left, Some(left), "at {now} ms");
        }
    }
}
