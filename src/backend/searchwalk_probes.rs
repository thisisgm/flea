// Construction of the shared walk probes, including the test seams.
use super::{Arc, AtomicBool, AtomicUsize, Mutex, WalkProbes};

impl WalkProbes {
    pub(crate) fn new() -> WalkProbes {
        WalkProbes {
            opened: AtomicUsize::new(0),
            cancel: Arc::new(AtomicBool::new(false)),
            watch: Mutex::new(None),
            #[cfg(test)]
            hold: Mutex::new(None),
            #[cfg(test)]
            read_hold: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            remote_reads: AtomicUsize::new(0),
            #[cfg(test)]
            body: Mutex::new(None),
            #[cfg(test)]
            landing_sent: AtomicBool::new(false),
            #[cfg(test)]
            prior_load: Mutex::new(None),
        }
    }
}
