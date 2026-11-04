//! A release uses its row identity once and keeps the completed result after the worker ends.
use super::*;

impl Service {
    pub fn unmount(
        &mut self,
        entry: &flea::ui_logic_pl1::RailEntry,
        listing: &str,
        origin: u64,
        cx: &mut Context<Self>,
    ) {
        if self.release.handle().is_some() {
            return;
        }
        let Some(job) = super::super::menus::unmount(entry, listing) else {
            return;
        };
        let now = self.now(cx);
        let effects = self.mounts.retire_location(&entry.uri);
        self.apply(effects, now, cx);
        let label = entry.label.clone();
        let handle = self.runner.start(job);
        let events = handle.events.clone();
        let run = self.release.start(handle);
        self.readers.retain(|reader| !reader.is_ready());
        self.readers.push(cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if let Event::Exited(out) = event {
                    let _ = this.update(cx, |this, cx| {
                        if !this.release.ended(run) {
                            return;
                        }
                        this.result = if out.code == 0 {
                            super::super::result::ResultState::Unmounted
                        } else {
                            super::super::result::ResultState::Failed
                        };
                        cx.emit(ServiceEvent(Effect::RefreshMounts));
                        let note = if out.code == 0 {
                            Note::new("network.unmounted", &[&label])
                        } else {
                            Note::new("network.unmount_failed", &[])
                        };
                        cx.emit(ServiceEvent(Effect::Say { note, origin }));
                    });
                    break;
                }
            }
        }));
    }
}
