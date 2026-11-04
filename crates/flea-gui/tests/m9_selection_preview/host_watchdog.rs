//! Bound native consumer waits so a blocked frame cannot hold the suite.
use super::*;

pub(super) struct Watchdog(Arc<std::sync::atomic::AtomicBool>);
impl Watchdog {
    pub(super) fn start() -> Self {
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let watch = stopped.clone();
        std::thread::spawn(move || {
            for _ in 0..600 {
                if watch.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            eprintln!("m9_selection_preview: a consumer frame blocked for 60 seconds");
            std::process::abort();
        });
        Self(stopped)
    }
}
impl Drop for Watchdog {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
