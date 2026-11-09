//! Retry and Edit carry their original pane and status into a card only when no draft owns it.
use super::{mount, view::NetworkView};
use gpui::{Context, Window};

impl NetworkView {
    pub fn completed(&mut self, completion: &mount::Effect, cx: &mut Context<Self>) -> bool {
        let mount::Effect::Completed {
            id,
            uri,
            error,
            args,
            connect_failed,
        } = completion
        else {
            return false;
        };
        if !self.model.opened
            || !self.model.connecting
            || self.model.request != *id
            || self.model.pending_uri != *uri
        {
            return false;
        }
        self.mount_finished(*id, uri, error.map_or(Ok(()), Err), *connect_failed, cx);
        self.model
            .set_detail(args.first().cloned().unwrap_or_default());
        self.changed(cx);
        true
    }
    pub fn location(
        &mut self,
        uri: &str,
        label: &str,
        edit: bool,
        origin: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.model.opened || self.model.busy() {
            return false;
        }
        let effects = self.model.open_location(uri, label, edit);
        if !self.model.opened {
            return false;
        }
        self.model.origin = origin;
        self.scroll = 0.;
        self.load_input();
        self.effects(effects, cx);
        window.focus(&self.focus, cx);
        true
    }
    pub fn retry(
        &mut self,
        request: mount::Effect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.model.opened || self.model.busy() {
            return false;
        }
        let mount::Effect::Retry {
            uri,
            label,
            origin,
            error,
            args,
            connect_failed,
            password,
        } = request
        else {
            return false;
        };
        if !self.location(&uri, &label, false, origin, window, cx) {
            return false;
        }
        self.model.password = password;
        if !self.model.failed_connect || connect_failed {
            self.model.status = Some(error);
            self.model.failed_connect = connect_failed;
            self.model.detail = args.first().cloned().unwrap_or_default();
        }
        self.load_input();
        self.changed(cx);
        true
    }
}
