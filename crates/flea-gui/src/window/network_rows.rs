//! The NETWORK rows: the gio listing polled while the rail polls, joined with mountinfo and saved places off the UI thread.
use super::*;

impl Shell {
    /// The poll asks only while the rail polls; a listing that overruns its deadline is killed and the last good one stands.
    pub(super) fn network_tick(&mut self, cx: &mut Context<Self>) {
        let polling = self
            .rail
            .as_ref()
            .is_some_and(|join| join.places.read(cx).polling);
        if !polling {
            return;
        }
        let Some(network) = self.network() else {
            return;
        };
        let service = network.service.clone();
        let now = service.update(cx, |service, cx| service.now(cx));
        let effects = self
            .network()
            .map(|n| n.listing.tick(now))
            .unwrap_or_default();
        self.network_listing(effects, cx);
    }

    /// An immediate re-list, after a mount lands, a share releases or a place is saved.
    pub(crate) fn network_list(&mut self, cx: &mut Context<Self>) {
        let Some(network) = self.network() else {
            return;
        };
        let service = network.service.clone();
        let now = service.update(cx, |service, cx| service.now(cx));
        let effects = self
            .network()
            .map(|n| n.listing.poll(now))
            .unwrap_or_default();
        self.network_listing(effects, cx);
    }

    fn network_listing(&mut self, effects: Vec<Listed>, cx: &mut Context<Self>) {
        for effect in effects {
            let Some(network) = self.network() else {
                return;
            };
            match effect {
                Listed::Kill => network.listing_slot.kill(),
                Listed::Listed => self.network_rows(cx),
                Listed::Start(job) => {
                    let handle = network.runner.start(job);
                    let events = handle.events.clone();
                    let run = network.listing_slot.start(handle);
                    let task = cx.spawn(async move |this, cx| {
                        while let Ok(event) = events.recv().await {
                            if let Proc::Exited(out) = event {
                                let _ = this.update(cx, |shell, cx| {
                                    let Some(network) = shell.network() else {
                                        return;
                                    };
                                    if !network.listing_slot.ended(run) {
                                        return;
                                    }
                                    let service = network.service.clone();
                                    let now = service.update(cx, |service, cx| service.now(cx));
                                    let next = shell
                                        .network()
                                        .map(|n| n.listing.exited(&out, now))
                                        .unwrap_or_default();
                                    shell.network_listing(next, cx);
                                });
                                break;
                            }
                        }
                    });
                    network.tasks.retain(|task| !task.is_ready());
                    network.tasks.push(task);
                }
            }
        }
    }

    /// mountinfo and the bookmarks file are read off the UI thread, then the rows rebuild from the last good listing.
    fn network_rows(&mut self, cx: &mut Context<Self>) {
        let Some(network) = self.network() else {
            return;
        };
        let gio = network.listing.text.clone();
        let path = network.bookmarks.clone();
        let read = cx.background_spawn(async move {
            let marks = std::fs::read_to_string(path).unwrap_or_default();
            let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
            let rows = flea::ui_logic_pl2::rail::rebuild(
                &gio,
                &flea::ui_logic_pl1::mounts::non_file_bookmarks(&marks),
                &mountinfo,
            );
            (rows, crate::path::aliases::saved_labels(&marks))
        });
        let task = cx.spawn(async move |this, cx| {
            let (rows, labels) = read.await;
            let _ = this.update(cx, |shell, cx| {
                if let Some(network) = shell.network() {
                    network.built = true;
                }
                if shell.path_saved_labels(labels) {
                    cx.notify();
                }
                shell.network_publish(Some(rows), cx)
            });
        });
        network.tasks.retain(|task| !task.is_ready());
        network.tasks.push(task);
    }

    /// Hands the NETWORK section to the rail's tail seam; Settings, Places, Network off hands it nothing.
    pub(crate) fn network_publish(&mut self, rows: Option<Vec<RailEntry>>, cx: &mut Context<Self>) {
        if let (Some(rows), Some(network)) = (rows, self.network()) {
            network.rows = rows;
        }
        self.rail_tail(cx);
    }

    /// The tail's one writer: NETWORK ahead of DEVICES, so neither owner's publish drops the other's section.
    pub(crate) fn rail_tail(&mut self, cx: &mut Context<Self>) {
        let state = self.launch.settings.data.clone();
        let Some(join) = &self.rail else {
            return;
        };
        let mut tail = join
            .network
            .as_ref()
            .map(|network| section(&network.rows, shown(&state)))
            .unwrap_or_default();
        tail.extend(join.devices.read(cx).sections.iter().cloned());
        join.places
            .update(cx, |producer, cx| producer.set_tail(tail, cx));
    }
}
