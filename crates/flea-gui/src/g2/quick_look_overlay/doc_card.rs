//! The card's surface for a mounted view: media fills the card under its strip; every other kind sits under the chrome bar.
use super::{
    card::recipe,
    chrome,
    doc::{Doc, View},
    look::{self, Look},
    Body, Outcome, QuickLook, Subject,
};
use crate::g2::media_preview::{self, ink::MediaInk};
use flea::ui_logic_media_preview::player::Playback;
use flea::ui_logic_preview_core::keys::Command;
use gpui::{div, prelude::*, AnyElement, App, Context, Window};

impl Doc {
    pub fn element(&self) -> AnyElement {
        match &self.view {
            View::Code(v) => v.clone().into_any_element(),
            View::Markdown(v) => v.clone().into_any_element(),
            View::Table(v) => v.clone().into_any_element(),
            View::Html(v) => v.clone().into_any_element(),
            View::Archive(v) => v.clone().into_any_element(),
            View::Office(v) => v.clone().into_any_element(),
            View::Font(v) => v.clone().into_any_element(),
            View::Pdf(v) => v.clone().into_any_element(),
            View::Model(v) => v.clone().into_any_element(),
            // The player draws through the card's media surface, never as an element of its own.
            View::Media(_) => div().into_any_element(),
        }
    }

    /// Subscribes the card to the view, so each change re-reads its readiness, shape and opening page.
    pub(super) fn observe(&self, cx: &mut Context<QuickLook>) -> gpui::Subscription {
        let changed = |ql: &mut QuickLook, cx: &mut Context<QuickLook>| ql.doc_changed(cx);
        match &self.view {
            View::Code(v) => cx.observe(v, move |ql, _, cx| changed(ql, cx)),
            View::Markdown(v) => cx.observe(v, move |ql, _, cx| changed(ql, cx)),
            View::Table(v) => cx.observe(v, move |ql, _, cx| changed(ql, cx)),
            View::Html(v) => cx.observe(v, move |ql, _, cx| changed(ql, cx)),
            View::Archive(v) => cx.observe(v, move |ql, _, cx| changed(ql, cx)),
            View::Office(v) => cx.observe(v, move |ql, _, cx| changed(ql, cx)),
            View::Font(v) => cx.observe(v, move |ql, _, cx| changed(ql, cx)),
            View::Pdf(v) => cx.observe(v, move |ql, _, cx| changed(ql, cx)),
            View::Model(v) => cx.observe(v, move |ql, _, cx| changed(ql, cx)),
            View::Media(m) => cx.observe(&m.player, move |ql, _, cx| changed(ql, cx)),
        }
    }
}

impl QuickLook {
    /// The calm card a refused code read draws in the mounted view's place (ql-unsupported): no bar, the mark and sentence in the metadata ink.
    pub(crate) fn calm_body(&self, cx: &App) -> Option<Body> {
        let doc = self.subject.as_ref()?.body.doc()?;
        let sentence = doc.calm_sentence(&self.i18n, cx)?;
        Some(Body::CannotShow {
            sentence,
            error: false,
            controls: Vec::new(),
        })
    }

    /// A mounted view is open, so it lays out even while the swap holds the card back.
    pub(super) fn has_doc(&self) -> bool {
        self.subject
            .as_ref()
            .is_some_and(|s| s.body.doc().is_some())
    }

    /// Cursor keys scroll a mounted PDF; other kinds keep the card's listing-follow rules.
    pub(super) fn doc_action(
        &mut self,
        doc: &Doc,
        action: &str,
        cx: &mut Context<Self>,
    ) -> Option<Command> {
        if matches!(action, "preview" | "escape")
            || (action == "focusPreview" && doc.pages(cx).is_some())
        {
            self.close(cx);
            return Some(Command::Close);
        }
        if matches!(action, "cursorDown" | "cursorUp") && doc.pages(cx).is_none() {
            return None;
        }
        let waiting = self.waiting(cx);
        let outcome = doc.action(action, cx);
        self.rearm_waiting(waiting, cx);
        match outcome {
            Outcome::Ignored => None,
            outcome => Some(self.follow_outcome(outcome, cx)),
        }
    }

    pub(super) fn follow_outcome(&mut self, outcome: Outcome, cx: &mut Context<Self>) -> Command {
        cx.notify();
        match outcome {
            Outcome::Close => {
                self.close(cx);
                Command::Close
            }
            Outcome::Expand(on) => {
                self.expanded = on;
                Command::ToggleExpand
            }
            Outcome::Taken | Outcome::Ignored => Command::Nothing,
        }
    }

    /// The mounted view moved: its page shape, the PDF's opening page, and the swap's reveal once it can draw.
    pub(super) fn doc_changed(&mut self, cx: &mut Context<Self>) {
        let Some(subject) = self.subject.as_mut() else {
            return;
        };
        let super::Body::Doc(doc) = &mut subject.body else {
            return;
        };
        doc.settle(cx);
        subject.shape = doc.shape(subject.state, cx);
        if doc.ready(cx) {
            self.observe_reveal(false, cx);
            self.mark_revealed(cx);
        }
        cx.notify();
    }

    pub(super) fn doc_parts(
        &self,
        subject: &Subject,
        doc: &Doc,
        look: &Look,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let i18n = self.i18n.clone();
        if let View::Media(media) = &doc.view {
            let frame = self.card(window.viewport_size());
            let (w, h) = crate::preview_quality::physical(
                f64::from(frame.size.width),
                f64::from(frame.size.height) - f64::from(media_preview::ink::size::STRIP),
                f64::from(window.scale_factor()),
            );
            media.player.update(cx, |player, _| player.resize(w, h));
            let theme = cx
                .try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default();
            // Preview.qml: audio's strip never hides; a video's shows while paused and on pointer contact.
            let paused = media.player.read(cx).state.playback != Playback::Playing;
            let weak = cx.entity().downgrade();
            let card = media_preview::surface::QuickLook {
                player: &media.player,
                known: &media.known,
                catalog: &i18n,
                ink: MediaInk::inherited(&window.text_style(), theme),
                typeface: media.typeface.clone(),
                caches: &media.caches,
                strip_shown: self.strip_shown || paused,
                touched: Some(std::rc::Rc::new(move |_, cx| {
                    let _ = weak.update(cx, |look, cx| look.reveal_strip(cx));
                })),
                // A press during the hold only reveals the card, as the ground rules.
                frame_click: self.revealed,
            }
            .build(cx);
            return vec![div()
                .debug_selector(|| "pv1-media".into())
                .size_full()
                .child(card)
                .child(super::observations::bounds(
                    &self.observations,
                    |observed, bounds| observed.surface = Some(bounds),
                ))
                .into_any_element()];
        }
        // ql-archive: no bar over the card; its own heading opens it, and a failure recipe sits at the top.
        let is_archive = matches!(doc.view, View::Archive(_));
        let body = match doc.failure(&i18n, cx) {
            Some(failure) => {
                let ink = if failure.error { look.error } else { look.meta };
                let button = super::button::open_with(self, look, window, cx);
                recipe(
                    failure.glyph,
                    ink,
                    &failure.sentence,
                    look.foreground,
                    Some(button),
                    if is_archive { 0. } else { look::BAR },
                )
            }
            // File content keeps its own layout in a mirrored window.
            None => crate::locale_geometry::unmirrored(
                div()
                    .debug_selector(|| "pv1-doc".into())
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_hidden()
                    .child(doc.element())
                    .child(super::observations::bounds(
                        &self.observations,
                        |observed, bounds| observed.surface = Some(bounds),
                    )),
            )
            .into_any_element(),
        };
        if is_archive {
            return vec![body];
        }
        let chrome = doc.chrome(&i18n, cx);
        let bar = chrome::bar(
            subject.mark_with_languages(self.languages),
            &subject.name,
            &chrome,
            !doc.framed(cx),
            look,
            self,
            cx,
        );
        vec![bar, body]
    }
}
