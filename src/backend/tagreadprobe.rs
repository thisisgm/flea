// Test-only observation pauses one real tag read per owned fixture without replacing its I/O.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, LazyLock, Mutex};
use std::time::Duration;

struct Hook {
    prefix: PathBuf,
    reads: AtomicUsize,
    parked: mpsc::Sender<()>,
    resume: Mutex<mpsc::Receiver<()>>,
}

fn hooks() -> &'static Mutex<Vec<Arc<Hook>>> {
    static HOOKS: LazyLock<Mutex<Vec<Arc<Hook>>>> = LazyLock::new(Default::default);
    &HOOKS
}

pub(crate) struct Hold {
    hook: Arc<Hook>,
    parked: mpsc::Receiver<()>,
    resume: mpsc::Sender<()>,
}

impl Hold {
    pub(crate) fn new(prefix: PathBuf) -> Self {
        let (parked, waiting) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let hook = Arc::new(Hook { prefix, reads: AtomicUsize::new(0), parked, resume: Mutex::new(resume) });
        hooks().lock().unwrap().push(Arc::clone(&hook));
        Self { hook, parked: waiting, resume: release }
    }

    pub(crate) fn wait(&self) {
        self.parked.recv_timeout(Duration::from_secs(10)).expect("the first real tag read parks");
    }

    pub(crate) fn release(&self) {
        let _ = self.resume.send(());
    }

    pub(crate) fn reads(&self) -> usize {
        self.hook.reads.load(Ordering::SeqCst)
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        self.release();
        hooks().lock().unwrap().retain(|hook| !Arc::ptr_eq(hook, &self.hook));
    }
}

pub(crate) fn observe(path: &Path) {
    let hook = hooks().lock().unwrap().iter().find(|hook| path.starts_with(&hook.prefix)).cloned();
    if let Some(hook) = hook {
        if hook.reads.fetch_add(1, Ordering::SeqCst) == 0 {
            let _ = hook.parked.send(());
            let _ = hook.resume.lock().unwrap().recv();
        }
    }
}
