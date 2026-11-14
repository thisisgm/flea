//! Opt-in latency probe credits only the exact commit the compositor reported presented.
use gpui::{App, FrameFeedbackEvent, FrameOutcome, Subscription, Window};
use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        LazyLock,
    },
    time::Duration,
};

// Linux CLOCK_MONOTONIC, the clock the compositor stamps presented frames with.
pub(crate) const CLOCK_MONOTONIC: i32 = 1;
// Draws and submitted frames a trace remembers; anything older can no longer be presented.
const KEPT: usize = 512;
extern "C" {
    fn clock_gettime(clock: i32, time: *mut [i64; 2]) -> i32;
}
pub(crate) fn now() -> Option<Duration> {
    let mut time = [0i64; 2];
    // SAFETY: Linux CLOCK_MONOTONIC and a writable timespec with the platform ABI.
    if unsafe { clock_gettime(CLOCK_MONOTONIC, &mut time) } != 0
        || time[0] < 0
        || !(0..1_000_000_000).contains(&time[1])
    {
        return None;
    }
    Some(Duration::new(time[0] as u64, time[1] as u32))
}
#[derive(Clone)]
pub(crate) struct Trace(Rc<RefCell<State>>);
struct State {
    id: u64,
    start: Duration,
    draws: VecDeque<(u64, bool, bool)>,
    pending: VecDeque<(u64, bool, bool)>,
    first: bool,
    full: bool,
    observer: Option<Subscription>,
}
impl Trace {
    pub fn begin(surface: &str, path: &str) -> Option<Self> {
        static ON: LazyLock<bool> =
            LazyLock::new(|| std::env::var_os("FLEA_PREVIEW_TRACE").is_some());
        static NEXT: AtomicU64 = AtomicU64::new(1);
        if !*ON {
            return None;
        }
        let start = now()?;
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        eprintln!("PREVIEW-QUALITY {{\"id\":{id},\"surface\":\"{surface}\",\"path\":\"{}\",\"stage\":\"begin\",\"clock\":{CLOCK_MONOTONIC}}}", flea::json::escape(path));
        Some(Self(Rc::new(RefCell::new(State {
            id,
            start,
            draws: VecDeque::new(),
            pending: VecDeque::new(),
            first: false,
            full: false,
            observer: None,
        }))))
    }
    pub fn observe(&self, window: &mut Window, cx: &mut App) {
        if self.0.borrow().observer.is_some() {
            return;
        }
        let weak = Rc::downgrade(&self.0);
        let observer = window.observe_frame_feedback(cx, move |event, _, _| {
            let Some(state) = weak.upgrade() else { return; };
            let mut state = state.borrow_mut();
            match event {
                FrameFeedbackEvent::Submitted { draw_id, frame_id } => {
                    if let Some((_, ready, full)) = state.draws.iter().find(|(id, _, _)| *id == draw_id).copied() {
                        if state.pending.len() == KEPT { state.pending.pop_front(); }
                        state.pending.push_back((frame_id, ready, full));
                    }
                }
                FrameFeedbackEvent::Terminal(terminal) => {
                    let Some(at) = state.pending.iter().position(|(id, _, _)| *id == terminal.frame_id) else { return; };
                    let (_, ready, full) = state.pending.remove(at).unwrap();
                    let FrameOutcome::Presented(presented) = terminal.outcome else { return; };
                    let (Some(time), Some(clock)) = (presented.timestamp, presented.clock_id) else { return; };
                    if clock as i64 != i64::from(CLOCK_MONOTONIC) { return; }
                    let Some(elapsed) = time.checked_sub(state.start) else { return; };
                    for (stage, emit) in [("first", ready && !state.first), ("full", ready && full && !state.full)] {
                        if emit { eprintln!("PREVIEW-QUALITY {{\"id\":{},\"stage\":\"{stage}\",\"ms\":{:.3},\"frame\":{},\"clock\":{CLOCK_MONOTONIC}}}", state.id, elapsed.as_secs_f64() * 1000., terminal.frame_id); }
                    }
                    state.first |= ready;
                    state.full |= ready && full;
                }
            }
        });
        self.0.borrow_mut().observer = observer;
    }
    pub fn stage(&self, window: &Window, ready: bool, full: bool) {
        let Some(draw) = window.current_draw_id() else {
            return;
        };
        let mut state = self.0.borrow_mut();
        if state.draws.back().is_some_and(|(id, _, _)| *id == draw) {
            return;
        }
        if state.draws.len() == KEPT {
            state.draws.pop_front();
        }
        state.draws.push_back((draw, ready, full));
    }
}
