//! Pane elapsed time uses the window executor when mounted; standalone model callers keep their explicit times.
use gpui::BackgroundExecutor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(crate) struct Clock {
    pub identity: u64,
    origin: Instant,
    executor: Option<BackgroundExecutor>,
}

impl Default for Clock {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            identity: NEXT.fetch_add(1, Ordering::Relaxed),
            origin: Instant::now(),
            executor: None,
        }
    }
}

impl Clock {
    pub fn at(&self, elapsed_ms: u64) -> Instant {
        self.origin + Duration::from_millis(elapsed_ms)
    }

    pub fn bind(&mut self, executor: BackgroundExecutor) {
        self.origin = executor.now();
        self.executor = Some(executor);
    }

    pub fn elapsed(&self) -> Duration {
        self.executor
            .as_ref()
            .map_or_else(Instant::now, BackgroundExecutor::now)
            .saturating_duration_since(self.origin)
    }
}
