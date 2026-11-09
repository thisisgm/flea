//! The pid of each AirDrop long run under its generation, so Stop signals only the run still current.
use super::airdrop::Proc;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};

// A run's pid once its thread has spawned it, and whether a Stop or a closing window came first.
#[derive(Default)]
pub(crate) struct Run {
    pid: Option<u32>,
    stopped: bool,
}

pub(crate) type Held = Arc<Mutex<Run>>;

#[derive(Default)]
pub(crate) struct Pids([Option<(u64, Held)>; Proc::ALL.len()]);

impl Pids {
    // Claimed before the spawn, so a Stop that comes first still reaches the run.
    pub(super) fn claim(&mut self, proc: Proc, generation: u64) -> Held {
        let run = Held::default();
        self.0[proc as usize] = Some((generation, run.clone()));
        run
    }

    pub(super) fn stop(&mut self, proc: Proc) {
        if let Some((_, run)) = self.0[proc as usize].take() {
            let mut run = run.lock().unwrap_or_else(PoisonError::into_inner);
            run.stopped = true;
            if let Some(pid) = run.pid {
                terminate(pid);
            }
        }
    }

    pub(super) fn ended(&mut self, proc: Proc, generation: u64) {
        if self.0[proc as usize]
            .as_ref()
            .is_some_and(|(held, _)| *held == generation)
        {
            self.0[proc as usize] = None;
        }
    }
}

// The spawning thread records its child here; a run stopped before it started is terminated at once.
pub(super) fn started(run: &Held, pid: u32) {
    let mut run = run.lock().unwrap_or_else(PoisonError::into_inner);
    run.pid = Some(pid);
    if run.stopped {
        terminate(pid);
    }
}

// SIGTERM lets `owly radio up` give Wi-Fi back; its own --for bounds it if Flea is gone first.
fn terminate(pid: u32) {
    std::thread::spawn(move || {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdin(Stdio::null())
            .status();
    });
}

impl Drop for Pids {
    // A window that closes with AirDrop on hands the radio back.
    fn drop(&mut self) {
        for proc in Proc::ALL {
            self.stop(proc);
        }
    }
}

#[cfg(test)]
#[path = "airdrop_pids_tests.rs"]
mod tests;
