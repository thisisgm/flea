//! The pid of each AirDrop long run under its generation, so Stop signals only the run still current.
use super::airdrop::Proc;
use std::process::{Command, Stdio};

#[derive(Default)]
pub(crate) struct Pids([Option<(u64, u32)>; Proc::ALL.len()]);

impl Pids {
    pub(super) fn hold(&mut self, proc: Proc, generation: u64, pid: u32) {
        self.0[proc as usize] = Some((generation, pid));
    }

    pub(super) fn stop(&mut self, proc: Proc) {
        if let Some((_, pid)) = self.0[proc as usize].take() {
            // SIGTERM lets `owly radio up` give Wi-Fi back; its own --for bounds it if Flea is gone first.
            std::thread::spawn(move || {
                let _ = Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .stdin(Stdio::null())
                    .status();
            });
        }
    }

    pub(super) fn ended(&mut self, proc: Proc, generation: u64) {
        if self.0[proc as usize].is_some_and(|(held, _)| held == generation) {
            self.0[proc as usize] = None;
        }
    }
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
