//! The NETWORK rows: the gio listing polled while the rail polls, joined with mountinfo and saved places off the UI thread.
use super::*;

impl Shell {
    /// Network and device rows consume the same complete gio listing, including its timeout verdict.
    pub(super) fn network_sync_mounts(&mut self, cx: &mut Context<Self>) {
        let Some(devices) = self.rail.as_ref().map(|join| join.devices.clone()) else {
            return;
        };
        let (generation, listing) = devices.read(cx).mount_snapshot();
        let Some(network) = self.network() else {
            return;
        };
        let changed = generation != network.listing_generation;
        network.listing = listing;
        network.listing_generation = generation;
        if changed {
            self.network_rows(cx);
        }
    }

    pub(crate) fn network_list(&mut self, cx: &mut Context<Self>) {
        let Some(devices) = self.rail.as_ref().map(|join| join.devices.clone()) else {
            return;
        };
        devices.update(cx, |source, cx| source.poll_mounts(cx));
        self.network_sync_mounts(cx);
    }

    /// A saved bookmark write landed: the rows and crumb labels rebuild from the file it just wrote.
    pub(super) fn network_saved(&mut self, cx: &mut Context<Self>) {
        if let Some(network) = self.network() {
            network.rows_generation += 1;
        }
        self.network_rows(cx);
    }

    /// mountinfo and the bookmarks file are read off the UI thread, then the rows rebuild from the last good listing.
    pub(super) fn network_rows(&mut self, cx: &mut Context<Self>) {
        self.network_rebuild(None, None, cx);
    }

    /// The poll and the IPC feed share one rebuild. A feed never claims the first real listing, and an older read never publishes over a newer one.
    pub(crate) fn network_rebuild(
        &mut self,
        mountinfo: Option<String>,
        answer: Option<std::sync::mpsc::SyncSender<crate::ipc::server::Answer>>,
        cx: &mut Context<Self>,
    ) {
        let Some(network) = self.network() else {
            return;
        };
        let gio = network.listing.text.clone();
        let path = network.bookmarks.clone();
        let sources = network.sources.clone();
        network.rows_generation += 1;
        let generation = network.rows_generation;
        let polled = mountinfo.is_none();
        let read = cx.background_executor().spawn(async move {
            let marks = std::fs::read_to_string(path).unwrap_or_default();
            let mountinfo = mountinfo.unwrap_or_else(|| sources.read_mountinfo());
            let ready = !mountinfo.is_empty();
            let rows = sources.load_rows(&gio, &marks, &mountinfo);
            (rows, crate::path::aliases::saved_labels(&marks), ready)
        });
        let task = cx.spawn(async move |this, cx| {
            let (rows, labels, ready) = read.await;
            let _ = this.update(cx, |shell, cx| {
                let current = match shell.network() {
                    Some(network) => {
                        network.built |= polled;
                        network.rows_generation == generation
                    }
                    None => false,
                };
                if !current {
                    return;
                }
                if let Some(network) = shell.network() {
                    network.mountinfo_ready = ready;
                }
                if shell.path_saved_labels(labels) {
                    cx.notify();
                }
                shell.network_publish(Some(rows), cx)
            });
            if let Some(answer) = answer {
                let reply = flea::ipc::wire::Reply::value("fed");
                let _ = answer.try_send(crate::ipc::server::Answer { reply, quit: false });
            }
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
        if let Some(host) = &self.plugins.host {
            let host = host.read(cx);
            if !host.plugins.is_empty() && !crate::g2::plugins::rail::dropbox_enabled(host) {
                for section in &mut tail {
                    section
                        .rows
                        .retain(|row| row.kind != flea::ui_logic_rail_places::rail::Kind::Dropbox);
                }
                tail.retain(|section| !section.rows.is_empty());
            }
        }
        tail.extend(join.devices.read(cx).sections.iter().cloned());
        if let Some(host) = &self.plugins.host {
            tail.extend(crate::g2::plugins::rail::sections(host.read(cx)));
        }
        join.places
            .update(cx, |producer, cx| producer.set_tail(tail, cx));
    }
}
