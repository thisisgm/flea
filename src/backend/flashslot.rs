// The flash slot and its answer channel, carried into the loop beside the transfer state.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// One flash runs at a time per backend; transfers keep their own slot beside it.
pub struct Slot(Mutex<Option<(usize, Arc<AtomicBool>)>>);

impl Slot {
    pub fn new() -> Slot {
        Slot(Mutex::new(None))
    }

    pub(crate) fn claim(&self, id: usize) -> Option<Arc<AtomicBool>> {
        let mut held = self.0.lock().ok()?;
        if held.is_some() {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        *held = Some((id, Arc::clone(&flag)));
        Some(flag)
    }

    pub fn running(&self) -> Option<usize> {
        self.0.lock().ok().and_then(|held| held.as_ref().map(|(id, _)| *id))
    }

    pub fn cancel(&self, id: usize) {
        if let Ok(held) = self.0.lock() {
            if let Some((running, flag)) = held.as_ref() {
                if *running == id {
                    flag.store(true, Ordering::Relaxed);
                }
            }
        }
    }

    pub(crate) fn finish(&self, id: usize) {
        if let Ok(mut held) = self.0.lock() {
            if held.as_ref().is_some_and(|(running, _)| *running == id) {
                *held = None;
            }
        }
    }
}

// One line the flash worker sends back; terminal releases the slot, the rest only print.
pub struct FlashMsg {
    pub id: usize,
    pub line: String,
    pub terminal: bool,
}

pub struct Ctx {
    pub slot: Slot,
    pub tx: super::lazy::DeferredSender<FlashMsg>,
}

impl Ctx {
    pub fn new(tx: std::sync::mpsc::Sender<FlashMsg>) -> Ctx {
        Ctx { slot: Slot::new(), tx: tx.into() }
    }

    // One answer, written and flushed; a terminal line releases the slot, the rest only print.
    pub fn report(&self, out: &mut impl std::io::Write, msg: &FlashMsg) {
        writeln!(out, "{}", msg.line).ok();
        out.flush().ok();
        if msg.terminal {
            self.slot.finish(msg.id);
        }
    }
}

pub fn spawn_forwarder(rx: std::sync::mpsc::Receiver<FlashMsg>, tx: crate::backend::events::Sender<crate::backend::events::Event>) {
    std::thread::spawn(move || {
        for msg in rx {
            if tx.send(crate::backend::events::Event::Flash(msg)).is_err() {
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_claim_while_one_runs_is_refused() {
        let slot = Slot::new();
        let _held = slot.claim(1).unwrap();
        assert!(slot.claim(2).is_none());
        assert_eq!(slot.running(), Some(1));
        slot.cancel(2);
        slot.finish(1);
        assert_eq!(slot.running(), None);
        assert!(slot.claim(2).is_some());
    }
}
