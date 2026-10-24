//! The device source's bounded process slots, driven by pl1's existing state machines.
use super::*;
use crate::g2::pl1::listing::Effect as ListingEffect;
use crate::g2::pl1::process::{Event, Job, Outcome};

const GIO: usize = 5;

impl Source {
    pub(super) fn apply(&mut self, effects: Vec<Effect>, cx: &mut Context<Self>) {
        self.apply_at(effects, self.mount_side, cx);
    }

    pub(super) fn apply_at(&mut self, effects: Vec<Effect>, side: usize, cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                Effect::Start(proc, job) => self.start(proc as usize, job, cx),
                Effect::Kill(proc) => self.slots[proc as usize].stop(),
                effect => {
                    self.events.borrow_mut().push(Request {
                        side,
                        event: super::Event::Device(effect),
                    });
                    cx.notify();
                }
            }
        }
        self.publish(cx);
        self.arm(cx);
    }

    pub(super) fn listing_effects(&mut self, effects: Vec<ListingEffect>, cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                ListingEffect::Start(job) => self.start(GIO, job, cx),
                ListingEffect::Kill => self.slots[GIO].stop(),
                ListingEffect::Listed => self.phones.rebuild(&self.listing.text),
            }
        }
    }

    fn start(&mut self, slot: usize, job: Job, cx: &mut Context<Self>) {
        let handle = self.runner.start(job);
        let events = handle.events.clone();
        let run = self.slots[slot].start(handle);
        self.readers.retain(|reader| !reader.is_ready());
        self.readers.push(cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if let Event::Exited(outcome) = event {
                    let _ = this.update(cx, |source, cx| source.exited(slot, run, outcome, cx));
                    break;
                }
            }
        }));
    }

    fn exited(&mut self, slot: usize, run: u64, outcome: Outcome, cx: &mut Context<Self>) {
        if !self.slots[slot].ended(run) {
            return;
        }
        let now = self.now(cx);
        if slot == GIO {
            let effects = self.listing.exited(&outcome, now);
            self.listing_effects(effects, cx);
        } else {
            let effects = match slot {
                0 => self.devices.list_exited(&outcome),
                1 => self.devices.mount_exited(&outcome, now),
                2 => self.devices.unmount_exited(&outcome, now),
                3 => self.devices.eject_exited(&outcome, now),
                4 => self.devices.lock_exited(&outcome, now),
                _ => Vec::new(),
            };
            self.apply(effects, cx);
        }
        self.publish(cx);
        self.arm(cx);
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        for slot in &mut self.slots {
            slot.kill();
        }
    }
}
