//! The render thread at a drop: queued work skipped, a drop that never waits on the UI thread, and finished threads joined.
use super::fixture::LETTER;
use super::renderer::next;
use super::{document, reaper_lock, root};
use flea_gui::g2::pv2::render::{reap, reaper_len, Renderer, Reply};
use std::time::{Duration, Instant};

#[test]
fn pv2_dropping_the_renderer_hands_its_thread_to_the_reaper_and_skips_the_queued_work() {
    let _reaper = reaper_lock();
    let root = root();
    let path = document(&root, "big.pdf", &[LETTER; 4]);
    let renderer = Renderer::spawn();
    let replies = renderer.replies.clone();
    let doc = renderer.open(path, true);
    assert!(
        matches!(next(&renderer), Reply::Opened { .. }),
        "the open answers first"
    );
    for turn in 0..100 {
        renderer.render(doc, turn % 4, 3000 + turn as u32);
    }
    let started = Instant::now();
    drop(renderer);
    let held = started.elapsed();
    assert!(
        held < Duration::from_secs(1),
        "the drop hands its thread over without waiting on it: {held:?}"
    );
    assert!(reaper_len() >= 1, "the dropped thread waits in the reaper");
    let started = Instant::now();
    reap();
    let joined = started.elapsed();
    assert!(
        replies.is_closed(),
        "after the reap the thread has ended, so no poppler or cairo call runs past it"
    );
    assert!(
        joined < Duration::from_secs(1),
        "the thread ends after at most the render in hand, not 100 queued: {joined:?}"
    );
}

#[test]
fn pv2_dropped_renderers_free_their_threads_without_waiting_for_exit() {
    let _reaper = reaper_lock();
    let root = root();
    let path = document(&root, "one.pdf", &[LETTER]);
    let before = reaper_len();
    // A Quick Look open and close drops one renderer per view.
    const VIEWS: usize = 50;
    for _ in 0..VIEWS {
        let renderer = Renderer::spawn();
        let doc = renderer.open(path.clone(), false);
        loop {
            match next(&renderer) {
                Reply::Opened { doc: opened, .. } if opened == doc => break,
                Reply::Opened { .. } => {}
                _ => panic!("the open answers first"),
            }
        }
        // The drop never waits: the FIX3 `pv2_a_renderer_drop_during_a_column_open_returns_at_once` test pins that bound.
        drop(renderer);
    }
    // Every worker above returns within its step of its drop; this waits them out.
    std::thread::sleep(Duration::from_secs(2));
    // One more drop sweeps every finished handle, so only threads still ending their step stay unjoined.
    let last = Renderer::spawn();
    let doc = last.open(path.clone(), false);
    loop {
        match next(&last) {
            Reply::Opened { doc: opened, .. } if opened == doc => break,
            Reply::Opened { .. } => {}
            _ => panic!("the open answers first"),
        }
    }
    drop(last);
    let waiting = reaper_len();
    println!("PV2_REAPER views={VIEWS} waiting={waiting}");
    reap();
    assert_eq!(reaper_len(), 0, "reap joins everything left");
    assert!(
        waiting <= before + 2,
        "{VIEWS} dropped views left {} unjoined threads until exit",
        waiting - before
    );
}

#[test]
fn pv2_a_renderer_drop_during_a_column_open_returns_at_once() {
    let _reaper = reaper_lock();
    for pages in [20_000, 200_000] {
        let root = root();
        let path = document(&root, "tree.pdf", &vec![LETTER; pages]);
        let renderer = Renderer::spawn();
        renderer.open(path, true);
        std::thread::sleep(Duration::from_millis(20));
        let started = Instant::now();
        drop(renderer);
        let held = started.elapsed();
        println!(
            "PV2_DROP pages={pages} held_ms={:.3}",
            held.as_secs_f64() * 1e3
        );
        reap();
        assert!(
            held < Duration::from_millis(10),
            "a {pages}-page column open held the UI thread's drop for {held:?}"
        );
    }
}
