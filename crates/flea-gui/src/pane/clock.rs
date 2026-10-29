//! Pane elapsed time uses the window executor when mounted; standalone model callers keep their explicit times.
use gpui::BackgroundExecutor;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(crate) struct Clock {
    pub identity: u64,
    origin: Instant,
    executor: Option<BackgroundExecutor>,
    /// Test seam: elapsed ms that only `advance_manual` moves; no timer runs.
    #[cfg(test)]
    manual: Option<Arc<AtomicU64>>,
}

impl Default for Clock {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            identity: NEXT.fetch_add(1, Ordering::Relaxed),
            origin: Instant::now(),
            executor: None,
            #[cfg(test)]
            manual: None,
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
        #[cfg(test)]
        if let Some(manual) = &self.manual {
            return Duration::from_millis(manual.load(Ordering::Relaxed));
        }
        self.executor
            .as_ref()
            .map_or_else(Instant::now, BackgroundExecutor::now)
            .saturating_duration_since(self.origin)
    }
}

#[cfg(test)]
impl Clock {
    /// Takes time from a counter at zero; the executor's clock and the wall clock play no part.
    pub fn use_manual(&mut self) {
        self.manual = Some(Arc::new(AtomicU64::new(0)));
    }

    pub fn advance_manual(&self, by: Duration) {
        if let Some(manual) = &self.manual {
            manual.fetch_add(by.as_millis() as u64, Ordering::Relaxed);
        }
    }
}
