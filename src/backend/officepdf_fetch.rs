// Print and export are durable consumers of the preview cache, never replaced by a later preview.
use super::*;

pub fn cached_pdf(path: &Path, cancel: Arc<AtomicBool>) -> Result<PathBuf, String> {
    production().fetch(path, cancel)
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
        self.enqueue(Job { key, program, input, mtime, seq: 0, waiters: vec![Waiter { wire: 0, reg: 0, tx }], required: true, cancel: Some(cancel) });
        let OpMsg::DetachedDone { line, .. } = rx.recv().map_err(|_| "the office converter stopped answering")? else {
            return Err("the office converter returned an invalid answer".into());
        };
        match crate::json::field_str(&line, "path") {
            Some(path) => Ok(PathBuf::from(path)),
            None => Err(crate::json::field_str(&line, "err").unwrap_or_else(|| "the office conversion failed".into())),
        }
    }
}
