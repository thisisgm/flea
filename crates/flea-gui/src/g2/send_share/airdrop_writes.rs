//! What a window changed in the machine's AirDrop state, written off its thread in the order it changed.
use super::{Shared, STATE, STOP};
use std::fs::File;
use std::io::Write as _;
use std::path::Path;

// A look took the lock, or found the shared directory unusable so the radio keeps to this window.
pub(crate) enum Claim {
    Lock(File),
    Local,
}

// One change to the machine's state, carried to the background executor with all it needs.
pub(crate) enum Write {
    // The owner's token, radio and card; a new owner's first one clears a stop asked of an earlier owner.
    State { body: String, fresh: bool },
    // The radio is going down: every window reads Off from here.
    Withdraw,
    // owly has exited, or an On did not use the lock its look took: the state goes first, then the lock.
    Release(File),
}

impl Write {
    pub(crate) fn run(self, dir: &Path) {
        let remove = |names: &[&str]| {
            for name in names {
                let _ = std::fs::remove_file(dir.join(name));
            }
        };
        match self {
            Write::State { body, fresh } => {
                if fresh {
                    remove(&[STOP]);
                }
                let temp = dir.join("airdrop.state.new");
                let written = File::create(&temp)
                    .and_then(|mut f| f.write_all(body.as_bytes()))
                    .and_then(|()| std::fs::rename(&temp, dir.join(STATE)));
                if let Err(error) = written {
                    eprintln!("flea: AirDrop could not share its state: {error}");
                }
            }
            Write::Withdraw => remove(&[STATE, STOP]),
            Write::Release(lock) => {
                remove(&[STATE, STOP]);
                drop(lock);
            }
        }
    }
}

impl Drop for Shared {
    // A closing window has no chain left, so its last writes run on a thread of their own after the ones under way.
    fn drop(&mut self) {
        self.release();
        let Some((dir, writes)) = self.writes() else {
            return;
        };
        let before = self.io.take();
        std::thread::spawn(move || {
            if let Some(before) = before {
                let _ = before.recv_blocking();
            }
            writes.into_iter().for_each(|write| write.run(&dir));
        });
    }
}
