//! Preview actions use the existing key resolver and the pane's retained generation; the preview family calls in here.
use super::selection_preview::{Mounted, Subject};
use crate::{state::Shell, window::input::ActionEvent};
use flea::jsondoc::Json;
use gpui::{Context, Window};

/// The preview keys that act on a shown preview only.
const QUIET: &[&str] = &[
    "previewPageForward",
    "previewPageBack",
    "factsToggle",
    "loadPreview",
    "focusPreview",
    "zoomIn",
    "zoomOut",
    "expand",
];

impl Shell {
    /// Runs a resolved action on the side's preview and answers whether it ran or was kept quiet.
    pub(crate) fn selection_preview_action(
        &mut self,
        side: usize,
        event: &ActionEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.focus_side != side || self.input_blocked(cx) {
            return false;
        }
        if event.action == "togglePreview" {
            let column =
                super::selection_preview::host::settings(&self.launch.settings.data).column;
            let leaf = Json::Obj(vec![("column".into(), Json::Bool(!column))]);
            let patch = Json::Obj(vec![("preview".into(), leaf)]);
            self.change_settings(patch, window, cx);
            return true;
        }
        // Focus.js:126-127 and the act() preview cases: with no shown preview these keys are quiet.
        let quiet = QUIET.contains(&event.action);
        let Some(host) = self.panes[side].as_ref().filter(|host| {
            !host.pane.busy()
                && !host.stopped
                && event.seat.listing == host.pane.held.listing as u64
        }) else {
            return quiet;
        };
        let Some(preview) = host.preview.clone().filter(|p| p.read(cx).visible) else {
            return quiet;
        };
        let subject = Subject::for_pane(host);
        match event.action {
            "loadPreview" => {
                preview.update(cx, |view, cx| view.follow(subject, false, true, window, cx))
            }
            "focusPreview" => {
                let focus = preview.read(cx).focus.clone();
                window.focus(&focus, cx);
                self.keyboard.context = preview.read(cx).key_context();
            }
            "escape" if preview.read(cx).focus.is_focused(window) => {
                window.focus(&self.focus, cx);
                self.keyboard.context = flea::keys::Context::Listing;
            }
            "preview" if preview.read(cx).focus.is_focused(window) => {
                preview.update(cx, |view, cx| view.play_pause(window, cx))
            }
            "factsToggle" => {
                let full = self
                    .launch
                    .settings
                    .data
                    .get("previewFacts")
                    .and_then(Json::as_str)
                    == Some("full");
                let facts = if full { "compact" } else { "full" };
                let patch = Json::Obj(vec![("previewFacts".into(), Json::Str(facts.into()))]);
                self.change_settings(patch, window, cx);
                return true;
            }
            // ql-pdf: e on the column's PDF opens Quick Look on the page the column shows.
            "expand" if column_pdf(&preview, cx).is_some() => {
                return self.open_quick_look(side, window, cx);
            }
            "previewPageForward" | "previewPageBack" | "parent" | "pageForward" | "seekBack"
            | "seekForward"
                if matches!(event.action, "previewPageForward" | "previewPageBack")
                    || (preview.read(cx).focus.is_focused(window)
                        && (!matches!(event.action, "seekBack" | "seekForward")
                            || column_pdf(&preview, cx).is_some())) =>
            {
                let step = if matches!(event.action, "parent" | "previewPageBack" | "seekBack") {
                    -1
                } else {
                    1
                };
                if let Some(pdf) = column_pdf(&preview, cx) {
                    pdf.update(cx, |pdf, cx| pdf.turn(step, cx));
                }
            }
            _ => return false,
        }
        self.sync_selection_previews(window, cx);
        cx.notify();
        true
    }
}

/// The column's mounted PDF, which its page keys turn and its expand opens at.
pub(crate) fn column_pdf(
    preview: &gpui::Entity<super::selection_preview::SelectionPreview>,
    cx: &gpui::App,
) -> Option<gpui::Entity<crate::g2::pv2::PdfView>> {
    preview
        .read(cx)
        .mounted_kind()
        .and_then(|(_, mounted)| match mounted {
            Mounted::Pdf(pdf) => Some(pdf.clone()),
            _ => None,
        })
}
