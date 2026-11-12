//! A real-backend fixture finishes its worker before its GPUI scheduler can retire.
use super::super::{Backend, Closing, Event};
use gpui::{App, Global, TestAppContext};
use std::{
    cell::RefCell,
    io::Write,
    os::unix::net::UnixStream,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

// Same grace as Backend::quit, and the bound on each fixture's worker join.
const FIXTURE_QUIT_GRACE: Duration = Duration::from_secs(30);

struct Fixtures(Rc<RefCell<Vec<Completion>>>);
impl Global for Fixtures {}

pub(crate) struct Completion {
    worker: Option<JoinHandle<()>>,
    closing: Arc<Mutex<Closing>>,
    wake: UnixStream,
    replies: async_channel::Receiver<Event>,
    pub(crate) joined: Arc<AtomicBool>,
}

impl Drop for Completion {
    fn drop(&mut self) {
        // Closing on the fixture thread wakes its receive tasks before any final worker sends.
        self.replies.close();
        let mut closing = self.closing.lock().unwrap();
        if *closing == Closing::Open {
            *closing = Closing::Quit(Instant::now() + FIXTURE_QUIT_GRACE);
        }
        drop(closing);
        let _ = (&self.wake).write(&[1]);
        let joined = self.worker.take().unwrap().join();
        // A second panic while a failed test unwinds would abort the whole test binary.
        if !std::thread::panicking() {
            joined.expect("backend fixture worker panicked");
        }
        self.joined.store(true, Ordering::SeqCst);
    }
}

impl Backend {
    pub(crate) fn test_completion(&self) -> Option<Completion> {
        Some(Completion {
            worker: Some(self.worker.lock().unwrap().take()?),
            closing: self.closing.clone(),
            wake: self.wake.try_clone().unwrap(),
            replies: self.replies.clone(),
            joined: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(crate) fn test_scope(cx: &mut TestAppContext) {
        if cx.update(|cx| cx.has_global::<Fixtures>()) {
            return;
        }
        let fixtures = Rc::new(RefCell::new(Vec::new()));
        let finishing = fixtures.clone();
        cx.on_quit(move || finishing.borrow_mut().clear());
        cx.update(|cx| cx.set_global(Fixtures(fixtures)));
    }

    pub(crate) fn test_own(&self, cx: &mut TestAppContext) -> Option<Arc<AtomicBool>> {
        Self::test_scope(cx);
        cx.update(|cx| self.test_adopt(cx))
    }

    pub(crate) fn test_adopt(&self, cx: &mut App) -> Option<Arc<AtomicBool>> {
        // A fake backend has no worker; a real one keeps its worker until the scope is proven.
        self.worker.lock().unwrap().as_ref()?;
        let fixtures = cx
            .try_global::<Fixtures>()
            .expect("real backend fixture needs Backend::test_scope")
            .0
            .clone();
        let completion = self.test_completion()?;
        let joined = completion.joined.clone();
        fixtures.borrow_mut().push(completion);
        Some(joined)
    }
}
