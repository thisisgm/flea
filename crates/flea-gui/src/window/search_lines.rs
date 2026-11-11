//! Backend search lines and listing landings reach the pane's search owner.
use super::*;

impl Shell {
    pub(crate) fn search_line(
        &mut self,
        side: usize,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let kind = flea::json::field_str(line, "t");
        if kind.as_deref() == Some("searchremap") {
            self.search_remapped(side, line, window, cx);
            return true;
        }
        if kind.as_deref() == Some("error")
            && flea::json::field_str(line, "where").as_deref() == Some("searchcontents")
        {
            let mut settled = false;
            self.search_update(
                side,
                None,
                |search, host| settled = search.contents_failed(line, host),
                window,
                cx,
            );
            if settled {
                let message = flea::json::field_str(line, "msg").unwrap_or_default();
                self.status_refused("searchcontents", &message, "", cx);
            }
            return true;
        }
        if !matches!(
            kind.as_deref(),
            Some("searchopened" | "searchclosed" | "searched" | "searching" | "contentsmatches")
        ) {
            return false;
        }
        let mut error = None;
        self.search_update(
            side,
            None,
            |search, host| {
                if let Err(message) = search.line(line, host) {
                    error = Some(message);
                }
            },
            window,
            cx,
        );
        if let (Some(message), Some(host)) = (error, self.panes[side].as_mut()) {
            host.record_error(
                crate::backend::Failure {
                    key: "window.backend_protocol",
                    detail: message,
                },
                &self.locale,
            );
        }
        true
    }

    pub(crate) fn search_landed(
        &mut self,
        side: usize,
        failed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = &self.panes[side] else {
            return;
        };
        if !host.search.owner.active() {
            return;
        }
        let failed = failed
            || matches!(
                host.pane.state.kind,
                crate::pane::states::Kind::Error | crate::pane::states::Kind::Locked
            );
        if !failed
            && (!host.search.owner.session.leaving || host.pane.busy() || host.pane.state.search)
        {
            return;
        }
        self.search_update(
            side,
            None,
            |search, host| {
                if failed {
                    search.leave(host);
                    search.listing_failed(host);
                }
                search.landed(host);
            },
            window,
            cx,
        );
    }
}
