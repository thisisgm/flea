// Print and export are durable consumers of the preview cache, never replaced by a later preview.
use super::*;

const CANCEL_POLL: Duration = Duration::from_millis(20);

pub fn cached_pdf(path: &Path, cancel: Arc<AtomicBool>) -> Result<PathBuf, String> {
    production().fetch(path, cancel)
}

// cached_pdf on a production-built service over a test's own cache root, so a real conversion never writes the user's cache.
#[cfg(test)]
pub(crate) fn cached_pdf_in(cache: &Path, path: &Path, cancel: Arc<AtomicBool>) -> Result<PathBuf, String> {
    service(cache).fetch(path, cancel)
}

impl Service {
    pub(super) fn fetch(self: &Arc<Self>, path: &Path, cancel: Arc<AtomicBool>) -> Result<PathBuf, String> {
        if cancel.load(Ordering::Relaxed) { return Err("cancelled".into()); }
        let program = (self.resolve)().ok_or("LibreOffice is not installed on this box.")?;
        let input = std::fs::canonicalize(path).map_err(|e| crate::error::io_message(&e))?;
        let (_, meta) = super::super::regfile::open_if_regular_with_meta(&input, 0).map_err(|e| crate::error::io_message(&e))?;
        let mtime = meta.modified().map_err(|e| crate::error::io_message(&e))?;
        let key = Store::key(path);
        match self.store.lookup(&key, mtime) {
            Hit::Ready(pdf) => return Ok(pdf),
            Hit::Failed(message) => return Err(message),
            Hit::Miss => {},
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.enqueue(Job { key, program, input, mtime, seq: 0, waiters: vec![Waiter { wire: 0, reg: 0, tx }], required: true, sheets: false, head: None, cancel: Some(Arc::clone(&cancel)) });
        let reply = loop {
            if cancel.load(Ordering::Relaxed) || self.quit.load(Ordering::Relaxed) {
                self.cancel_fetch(&cancel);
                return Err("cancelled".into());
            }
            match rx.recv_timeout(CANCEL_POLL) {
                Ok(reply) => break reply,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {},
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Err("the office converter stopped answering".into()),
            }
        };
        let OpMsg::DetachedDone { line, .. } = reply else {
            return Err("the office converter returned an invalid answer".into());
        };
        match crate::json::field_str(&line, "path") {
            Some(path) => Ok(PathBuf::from(path)),
            None => Err(crate::json::field_str(&line, "err").unwrap_or_else(|| "the office conversion failed".into())),
        }
    }

    fn cancel_fetch(&self, cancel: &Arc<AtomicBool>) {
        let mut q = self.locked();
        let index = q.required.iter().position(|job| job.cancel.as_ref().is_some_and(|flag| Arc::ptr_eq(flag, cancel)));
        let job = index.and_then(|index| q.required.remove(index));
        drop(q);
        if let Some(job) = job { answer(job.waiters, false, Err("cancelled".into())); }
    }
}
