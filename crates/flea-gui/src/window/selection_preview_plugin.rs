//! A previewer plugin answers for a file no built-in preview draws, through the shared plugin host.
use super::*;
use flea::ui_logic_st2::wire::Source;
use gpui::{AppContext, Window};

/// Enabled, unfailed third-party plugins that declare a previewer, in listing order.
pub(crate) fn previewers(host: &crate::g2::plugins::Host) -> Vec<String> {
    host.plugins
        .iter()
        .filter(|p| p.enabled && p.failure.is_none() && p.source != Source::Builtin)
        .filter(|p| p.manifest.block("previewer").is_some())
        .map(|p| p.manifest.id.clone())
        .collect()
}

impl SelectionPreview {
    // Only a local file whose built-in frame is the plain facts frame asks; the frame holds until the answer.
    // Sample input: {"op":"preview","text":"Title: Notes"} or {"op":"preview","png":"/run/user/1000/flea/preview.png"}
    pub(super) fn ask_previewer(
        &mut self,
        frame: &mut Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let host = crate::g2::plugins::service(cx);
        if self.previewer.0.is_none() {
            // A listing that lands after the frame (a restored selection at launch) still gets its ask.
            self.previewer.0 = Some(cx.observe_in(&host, window, |view, _, window, cx| {
                view.previewers_changed(window, cx)
            }));
        }
        if frame.manual
            || frame.remote
            || frame.kind != Kind::Facts
            || !matches!(frame.mounted, Mounted::Basic)
        {
            return;
        }
        if frame.generation == self.previewer.1 || frame.source.is_some() {
            return;
        }
        let plugins = previewers(host.read(cx));
        if plugins.is_empty() {
            return;
        }
        self.previewer.1 = frame.generation;
        let (width, height) = self.frame_size(Kind::Image, 1);
        let request = Json::Obj(vec![
            ("op".into(), Json::Str("preview".into())),
            (
                "paths".into(),
                Json::Arr(vec![Json::Str(frame.subject.path())]),
            ),
            ("width".into(), Json::Num(width.round().max(1.).to_string())),
            (
                "height".into(),
                Json::Num(height.round().max(1.).to_string()),
            ),
        ]);
        let generation = frame.generation;
        frame.thumb_answered = false;
        cx.spawn_in(window, async move |this, cx| {
            let mut answer = None;
            for plugin in plugins {
                let asked = cx
                    .update(|_, cx| {
                        host.update(cx, |h, cx| h.request_wait(&plugin, request.clone(), cx))
                    })
                    .ok()
                    .flatten();
                let Some(receiver) = asked else { break };
                if let Ok(response) = receiver.recv().await {
                    if response.ok
                        && response.message.get("op").and_then(Json::as_str) == Some("preview")
                    {
                        answer = Some(response.message);
                        break;
                    }
                }
            }
            let _ = this.update_in(cx, |view, window, cx| {
                view.previewer_answered(generation, answer, window, cx)
            });
        })
        .detach();
    }

    // The shown or pending frame asks once the host lists a previewer it did not list before.
    fn previewers_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for pending in [true, false] {
            let slot = if pending {
                &mut self.pending
            } else {
                &mut self.drawn
            };
            let Some(mut frame) = slot.take() else {
                continue;
            };
            self.ask_previewer(&mut frame, window, cx);
            let slot = if pending {
                &mut self.pending
            } else {
                &mut self.drawn
            };
            *slot = Some(frame);
            if pending {
                return;
            }
        }
    }

    // Sample input, the owner's answer with its picture moved out of the jail: {"op":"preview","text":"Title: Notes"} or {"op":"preview","png":"/run/user/1000/flea/preview.png"}
    fn previewer_answered(
        &mut self,
        generation: u64,
        answer: Option<Json>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The answer lands on its own frame, pending or already drawn.
        let pending = self
            .pending
            .as_ref()
            .is_some_and(|f| f.generation == generation);
        let shown = if pending { &self.pending } else { &self.drawn };
        let look = shown
            .as_ref()
            .filter(|f| f.generation == generation)
            .map(|f| self.doc_look(&f.subject, window, cx));
        let slot = if pending {
            &mut self.pending
        } else {
            &mut self.drawn
        };
        let Some(frame) = slot.as_mut().filter(|f| f.generation == generation) else {
            return;
        };
        frame.thumb_answered = true;
        let text = |key| {
            answer
                .as_ref()
                .and_then(|a| a.get(key))
                .and_then(Json::as_str)
        };
        if let (Some(text), Some(look)) = (text("text"), look) {
            // Plain text only, drawn by the same text view a built-in text preview uses.
            let path = frame.subject.path();
            let text = text.to_owned();
            let catalog = self.catalog.clone();
            let view = cx.new(|cx| {
                let mut view = look.code(false);
                view.max_bytes = look.text_limit;
                view.set_catalog(catalog, cx);
                view.set_active(true, cx);
                view.set_subject(&path, text.len() as u64, true, Some(text), cx);
                view
            });
            frame.mounted = Mounted::Doc(docs::Doc::Code(view));
        } else if let Some(png) = text("png").filter(|p| p.starts_with('/')) {
            frame.source = Some(std::path::PathBuf::from(png).into());
        }
        // The next frame's poll loads a returned picture; use_asset only runs while drawing.
        cx.notify();
    }
}
