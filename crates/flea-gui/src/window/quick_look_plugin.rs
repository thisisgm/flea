//! Quick Look asks a previewer plugin for a file it cannot show itself; the answer replaces the refusal card.
use super::super::docs::Look;
use crate::{
    g2::quick_look_overlay::{
        doc::{Doc, View},
        Body, Subject,
    },
    state::Shell,
};
use flea::{jsondoc::Json, protocol::RowFacts, ui_logic_preview_core::kinds::State};
use gpui::{AppContext, Context, Window};

impl Shell {
    /// The card's refusal for a row it cannot show; an enabled previewer plugin is asked to replace it.
    pub(in crate::window::quick_look) fn quick_look_unsupported(
        &mut self,
        row: &RowFacts,
        path: &str,
        token: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Body {
        self.quick_look_previewer(row, path, token, window, cx);
        Body::cannot_show(
            &self.locale.catalog,
            State::Unsupported,
            false,
            false,
            Vec::new(),
        )
        .expect("an unsupported kind has a sentence")
    }

    // Only a local row asks; a later subject or a closed card drops the answer.
    fn quick_look_previewer(
        &mut self,
        row: &RowFacts,
        path: &str,
        token: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let host = crate::g2::plugins::service(cx);
        let plugins = crate::window::selection_preview::plugin::previewers(host.read(cx));
        if plugins.is_empty() || !path.starts_with('/') {
            return;
        }
        let viewport = window.viewport_size();
        let request = Json::Obj(vec![
            ("op".into(), Json::Str("preview".into())),
            ("paths".into(), Json::Arr(vec![Json::Str(path.to_owned())])),
            (
                "width".into(),
                Json::Num(
                    f32::from(viewport.width)
                        .round()
                        .clamp(1., 8192.)
                        .to_string(),
                ),
            ),
            (
                "height".into(),
                Json::Num(
                    f32::from(viewport.height)
                        .round()
                        .clamp(1., 8192.)
                        .to_string(),
                ),
            ),
        ]);
        let (row, path) = (row.clone(), path.to_owned());
        cx.spawn_in(window, async move |this, cx| {
            for plugin in plugins {
                let asked = cx
                    .update(|_, cx| {
                        host.update(cx, |h, cx| h.request_wait(&plugin, request.clone(), cx))
                    })
                    .ok()
                    .flatten();
                let Some(receiver) = asked else { return };
                let Ok(response) = receiver.recv().await else {
                    continue;
                };
                if response.ok
                    && response.message.get("op").and_then(Json::as_str) == Some("preview")
                {
                    let _ = this.update_in(cx, |shell, window, cx| {
                        shell.quick_look_plugin_answer(
                            &row,
                            &path,
                            token,
                            &response.message,
                            window,
                            cx,
                        )
                    });
                    return;
                }
            }
        })
        .detach();
    }

    fn quick_look_plugin_answer(
        &mut self,
        row: &RowFacts,
        path: &str,
        token: u64,
        answer: &Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.quick_look.as_ref().is_some_and(|h| h.token == token) || !self.quick_look_open(cx)
        {
            return;
        }
        let number = |key| answer.get(key).and_then(Json::as_f64).unwrap_or(0.);
        let subject = |state, body| Subject {
            name: row.name.clone(),
            icon: row.icon.clone(),
            mode: row.mode,
            state,
            shape: flea::ui_logic_preview_core::rect::Shape::unknown(state),
            body,
        };
        let subject = if let Some(text) = answer.get("text").and_then(Json::as_str) {
            // Plain text only, in the same text view the card mounts for a text file.
            let theme = cx
                .try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default();
            let look = Look {
                theme,
                face: self.active_typeface(),
                muted: crate::list::Palette::inherited(&window.text_style(), theme).metadata,
                mark: super::mark(row),
            };
            let (text, catalog) = (text.to_owned(), self.locale.catalog.clone());
            let code = cx.new(|cx| {
                let mut view = look.code(false);
                view.set_catalog(catalog, cx);
                view.set_active(true, cx);
                view.set_subject(path, text.len() as u64, true, Some(text), cx);
                view
            });
            let doc = Doc {
                view: View::Code(code),
                path: path.to_owned(),
                page: 0,
            };
            let mut subject = subject(State::Text, Body::Doc(doc.clone()));
            subject.shape = doc.shape(State::Text, cx);
            subject
        } else if let Some(png) = answer
            .get("png")
            .and_then(Json::as_str)
            .filter(|p| p.starts_with('/'))
        {
            subject(
                State::Image,
                Body::Image {
                    path: png.into(),
                    w: number("width"),
                    h: number("height"),
                },
            )
        } else {
            return;
        };
        if let Some(host) = self.quick_look.as_mut() {
            host.ask = super::super::Ask::Nothing;
        }
        self.show(subject, true, None, window, cx);
    }
}
