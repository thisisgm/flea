//! The oracle's installed check starts on open, stays off-thread and retires with its dialog.
use super::{
    dialog::{Dialog, Effect},
    service::Service,
    view::NetworkView,
};
use crate::g2::device_rail::process::{Event, Job};
use gpui::{Context, EventEmitter};
use std::time::Duration;

pub struct ProviderStatus {
    pub id: u64,
    pub installed: bool,
}
impl EventEmitter<ProviderStatus> for Service {}
impl Dialog {
    pub fn provider_status(&mut self, id: u64, installed: bool) -> bool {
        if !self.opened || self.provider_check != Some(id) {
            return false;
        }
        self.provider_check = None;
        self.dropbox_installed = installed;
        if installed && self.focus == super::dialog::Focus::Dropbox {
            self.focus = super::dialog::Focus::Connect;
        }
        true
    }
    pub fn install_dropbox(&mut self) -> Vec<Effect> {
        if !self.opened || self.busy() || self.dropbox_installed {
            return Vec::new();
        }
        let mut effects = vec![Effect::InstallDropbox];
        effects.extend(self.close());
        effects
    }
}
impl NetworkView {
    pub fn provider_status(&mut self, status: &ProviderStatus, cx: &mut Context<Self>) {
        if self.model.provider_status(status.id, status.installed) {
            self.fit_focus();
            self.changed(cx);
        }
    }
}
impl Service {
    pub(super) fn stop_provider(&mut self) {
        self.provider.kill();
        self.provider_id = None;
        self.provider_timer = None;
    }
    pub(super) fn check_provider(&mut self, id: u64, cx: &mut Context<Self>) {
        self.stop_provider();
        self.provider_id = Some(id);
        let handle = self
            .runner
            .start(Job::c_locale(vec!["which".into(), "dropbox-cli".into()]));
        let events = handle.events.clone();
        let run = self.provider.start(handle);
        self.readers.retain(|reader| !reader.is_ready());
        self.readers.push(cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if let Event::Exited(out) = event {
                    let _ = this.update(cx, |this, cx| {
                        if this.provider_id == Some(id) && this.provider.ended(run) {
                            this.provider_id = None;
                            this.provider_timer = None;
                            cx.emit(ProviderStatus {
                                id,
                                installed: out.code == 0,
                            });
                        }
                    });
                    break;
                }
            }
        }));
        self.provider_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;
            let _ = this.update(cx, |this, cx| {
                if this.provider_id == Some(id) {
                    this.stop_provider();
                    cx.emit(ProviderStatus {
                        id,
                        installed: false,
                    });
                }
            });
        }));
    }
}
