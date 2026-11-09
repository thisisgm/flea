//! Review round 1 proofs: an all-hit table frame allocates nothing, Quick Look shapes only the columns in view, the
//! sniff never waits on a fifo, and a ready subject that cannot draw projects the failure sentence.
use super::*;
use gpui::{point, AppContext as _, Bounds};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
thread_local! { static ARMED: Cell<bool> = const { Cell::new(false) }; }

fn armed() -> bool {
    ARMED.try_with(Cell::get).unwrap_or(false)
}

// SAFETY: every call forwards to the system allocator unchanged; only a counter is added.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if armed() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        if armed() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if armed() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

// One layout of the frame with the counter armed: its allocations, the shaped count after it and its time.
fn counted(
    cx: &mut TestAppContext,
    view: WindowHandle<CsvView>,
    w: f32,
    h: f32,
) -> (usize, usize, Duration) {
    view.update(cx, |v, window, _| {
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(w), px(h)));
        let t = Instant::now();
        ARMED.with(|a| a.set(true));
        let before = ALLOCS.load(Ordering::Relaxed);
        v.layout(bounds, window);
        let n = ALLOCS.load(Ordering::Relaxed) - before;
        ARMED.with(|a| a.set(false));
        (n, v.cells.shaped, t.elapsed())
    })
    .unwrap()
}

// GPUI rule 2: a frame whose every cell is cached allocates nothing, at rest and scrolled back to cached rows.
#[gpui::test]
fn pv2_fix1_all_hit_table_frame_allocates_nothing(cx: &mut TestAppContext) {
    for look in [true, false] {
        let (view, sent) = table(cx, look, 902., 625.);
        view.update(cx, |v, _, cx| v.set_path("/d/budget.csv", cx))
            .unwrap();
        serve(cx, view, &sent, &budget());
        draw(cx, view);
        let shaped = read(cx, view, |v| v.cells.shaped);
        let (allocs, after, took) = counted(cx, view, 902., 625.);
        println!("FIX1 all-hit frame look={look}: {allocs} allocations, shaped {shaped} then {after}, {took:?}");
        assert_eq!(after, shaped, "every cell of the frame was a cache hit");
        assert_eq!(allocs, 0, "an all-hit frame allocated {allocs} times");
    }
}

// Quick Look lays out and shapes only the columns its frame shows, so a 50,000-column file costs a bounded frame.
#[gpui::test]
fn pv2_fix1_quick_look_shapes_only_the_columns_in_view(cx: &mut TestAppContext) {
    let cols = 50_000;
    let mut text = (1..=cols)
        .map(|c| format!("c{c}"))
        .collect::<Vec<_>>()
        .join(",");
    text.push('\n');
    for _ in 0..4 {
        text.push_str(&vec!["1"; cols].join(","));
        text.push('\n');
    }
    assert!(text.len() < 1 << 20, "under the backend's 1 MiB gate");
    let (view, sent) = table(cx, true, 460., 600.);
    view.update(cx, |v, _, cx| v.set_path("/d/wide.csv", cx))
        .unwrap();
    let t = Instant::now();
    serve(cx, view, &sent, &text);
    let first = t.elapsed();
    let (head, visible, cached) = read(cx, view, |v| {
        let visible = v
            .head
            .iter()
            .filter(|c| c.x < 460. && c.x + c.w > 0.)
            .count();
        (v.head.len(), visible, v.cells.len())
    });
    let (allocs, _, frame) = counted(cx, view, 460., 600.);
    println!("FIX1 Quick Look: first screen {first:?}; header cells {head}, visible {visible}, cache entries {cached}; an all-hit frame took {frame:?} and allocated {allocs} times");
    assert!(
        head <= visible + 1,
        "{head} header cells laid out and shaped for {visible} visible columns"
    );
    assert!(cached <= 5 * (visible + 2), "{cached} cells cached");
    assert_eq!(allocs, 0, "an all-hit wide frame allocated {allocs} times");
    view.update(cx, |v, _, cx| {
        v.scroll.0 = 1e9;
        cx.notify()
    })
    .unwrap();
    draw(cx, view);
    let (right, last) = read(cx, view, |v| {
        (v.head.len(), v.drawn_texts().next().map(str::to_string))
    });
    assert!(
        right <= visible + 2,
        "{right} header cells at the right end"
    );
    assert!(last.is_some(), "the right end draws its columns");
}

// The backend times out on a fifo named report.odt; the view's sniff must never wait for a writer.
#[gpui::test]
fn pv2_fix1_sniff_never_waits_on_a_fifo(cx: &mut TestAppContext) {
    let root = root();
    let fifo = root.0.join("report.odt");
    assert!(Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap()
        .success());
    let (view, _, _) = office(cx, true);
    open(cx, view, fifo.to_str().unwrap());
    // A writer 3 s later is the only thing that ends a blocked open; O_NONBLOCK so it never waits itself.
    let writer = {
        let fifo = fifo.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(3));
            let _ = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(0o4000)
                .open(&fifo);
        })
    };
    let t = Instant::now();
    reply(
        cx,
        view,
        r#"{"t":"officepdf","id":1,"state":"failed","err":"the conversion took longer than 60 seconds"}"#,
    );
    let waited = t.elapsed();
    writer.join().unwrap();
    println!("FIX1 fifo: the failed reply and its sniff returned after {waited:?}");
    assert!(
        waited < Duration::from_secs(1),
        "the sniff blocked on the fifo for {waited:?}"
    );
    assert_eq!(
        state(cx, view),
        State::Failed(flea::ui_logic_doc_preview::office::Reason::TimedOut(60))
    );
}

// A converted subject that cannot draw: its PDF does not open, or the engine refuses its sheet as too large.
#[gpui::test]
fn pv2_fix1_ready_subject_that_cannot_draw_says_why(cx: &mut TestAppContext) {
    let (view, _, code) = office(cx, true);
    let i18n = crate::test_catalogs::english();
    let sentence = Some(i18n.tr("document.office_failed"));
    open(cx, view, "/d/garden-report.docx");
    reply(
        cx,
        view,
        r#"{"t":"officepdf","id":1,"state":"ready","path":"/nonexistent/fix1/garden.pdf"}"#,
    );
    let mut failed = false;
    for _ in 0..400 {
        failed = view
            .update(cx, |v, _, cx| v.pdf.read(cx).surface.failed())
            .unwrap();
        if failed {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.update_window(view.into(), |_, window, cx| window.simulate_next_frame(cx))
            .unwrap();
        draw(cx, view);
    }
    draw(cx, view);
    let (shown, failure, diagnostic) = view
        .update(cx, |v, _, cx| {
            (
                v.shown(cx),
                v.failure(&i18n).map(|f| f.sentence),
                v.office.diagnostic.clone(),
            )
        })
        .unwrap();
    println!("FIX1 pdf: surface failed {failed}, shown {shown}, failure {failure:?}, diagnostic {diagnostic:?}");
    assert!(failed, "the PDF surface reports that its file did not open");
    assert!(shown, "the swap may show the failure");
    assert_eq!(
        failure, sentence,
        "a PDF that does not open says the plan's sentence"
    );
    assert_eq!(diagnostic, flea::ui_logic_doc_preview::office::PDF_UNOPENED);

    open(cx, view, "/d/big.xlsx");
    reply(
        cx,
        view,
        r#"{"t":"officesheets","id":2,"state":"ready","names":["Data"],"paths":["/c/d.csv"]}"#,
    );
    let ask = code
        .borrow()
        .last()
        .cloned()
        .expect("the sheet asked the engine");
    let token = field_usize(&ask, "token").unwrap();
    let large = format!(
        r#"{{"t":"code","token":{token},"slot":"look","state":"large","first":1,"total":0,"lines":[]}}"#
    );
    let table = view.update(cx, |v, _, _| v.table.clone()).unwrap();
    cx.update(|cx| table.update(cx, |t, cx| t.answered(&large, cx)));
    cx.run_until_parked();
    draw(cx, view);
    let (shown, failure, diagnostic) = view
        .update(cx, |v, _, cx| {
            (
                v.shown(cx),
                v.failure(&i18n).map(|f| f.sentence),
                v.office.diagnostic.clone(),
            )
        })
        .unwrap();
    println!("FIX1 sheet: shown {shown}, failure {failure:?}, diagnostic {diagnostic:?}");
    assert!(shown, "the swap may show the failure");
    assert_eq!(
        failure, sentence,
        "a refused sheet says the plan's sentence"
    );
    assert_eq!(
        diagnostic,
        flea::ui_logic_doc_preview::office::SHEET_REFUSED
    );
}

// The backend replaced the request with another client's: the view asks once more and stays converting.
#[gpui::test]
fn pv2_fix1_a_replaced_request_is_asked_again(cx: &mut TestAppContext) {
    let (view, asked, _) = office(cx, true);
    open(cx, view, "/d/talk.odp");
    let replaced = |id: u64| {
        format!(
            r#"{{"t":"officepdf","id":{id},"state":"failed","err":"replaced by a newer request"}}"#
        )
    };
    assert!(reply(cx, view, &replaced(1)));
    assert_eq!(
        state(cx, view),
        State::Converting,
        "a replaced request keeps converting"
    );
    assert_eq!(asked.borrow().len(), 2, "the view asked once more");
    assert!(asked.borrow()[1].contains(r#""id":2"#));
    assert!(reply(cx, view, &replaced(2)));
    assert_eq!(
        state(cx, view),
        State::Cancelled,
        "a second replace ends the request"
    );
    assert_eq!(asked.borrow().len(), 2, "no third ask");
}
