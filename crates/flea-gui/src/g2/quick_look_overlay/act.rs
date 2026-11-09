//! What the card does with a control press and with each key-table action (PreviewKeys.act).
use super::*;

impl QuickLook {
    /// A card control answered; a disabled one never reaches here.
    pub fn activate(&mut self, hit: Hit, cx: &mut Context<Self>) {
        if self.subject.is_none() {
            return;
        }
        match hit {
            Hit::Close => self.close(cx),
            Hit::OpenWith => cx.emit(Event::OpenWith),
            Hit::CopyPatch => cx.emit(Event::CopyPatch),
            Hit::Word(index) => {
                if let Some(doc) = self.subject.as_ref().and_then(|s| s.body.doc()).cloned() {
                    doc.word(index, cx);
                    cx.notify();
                }
            }
            Hit::Strip(index) => {
                if let Some(doc) = self.subject.as_ref().and_then(|s| s.body.doc()).cloned() {
                    let outcome = doc.press(index, cx);
                    self.follow_outcome(outcome, cx);
                }
            }
            Hit::Control(index) => {
                let glyph = self
                    .subject
                    .as_ref()
                    .and_then(|s| s.body.controls().get(index).copied());
                if glyph.is_some_and(|c| c.enabled && c.glyph == glyph::Glyph::Maximize) {
                    self.expanded = !self.expanded;
                    cx.notify();
                }
            }
        }
    }

    /// The PDF strip's walk when PdfViewer holds the keyboard: its controls, then Open With, the pane's one action, then Close.
    pub(super) fn strip(&self) -> Option<Vec<(Hit, bool)>> {
        let controls = self.subject.as_ref()?.body.controls();
        if controls.is_empty() {
            return None;
        }
        let mut walk: Vec<(Hit, bool)> = controls
            .iter()
            .enumerate()
            .map(|(i, c)| (Hit::Control(i), c.enabled))
            .collect();
        walk.extend([(Hit::OpenWith, true), (Hit::Close, true)]);
        Some(walk)
    }

    /// PdfViewer's keys (PreviewKeys.pdfAction); Space and Escape still close every kind.
    pub(super) fn strip_action(
        &mut self,
        walk: &[(Hit, bool)],
        action: &str,
        cx: &mut Context<Self>,
    ) -> Command {
        if matches!(action, "preview" | "escape" | "focusPreview") {
            self.close(cx);
            return Command::Close;
        }
        let controls: Vec<keys::Control> = walk
            .iter()
            .map(|&(_, enabled)| keys::Control {
                enabled,
                visible: true,
            })
            .collect();
        let index = self
            .focused
            .and_then(|f| walk.iter().position(|(h, _)| *h == f))
            .map_or(-1, |i| i as i32);
        match keys::pdf_action(action, Some(&controls), index) {
            PdfCommand::Focus(i) => {
                self.focused = walk.get(i as usize).map(|(h, _)| *h);
                cx.notify();
                Command::Nothing
            }
            PdfCommand::Activate(i) => {
                self.activate(walk[i].0, cx);
                Command::Nothing
            }
            PdfCommand::ToggleExpand => {
                self.expanded = !self.expanded;
                cx.notify();
                Command::ToggleExpand
            }
            // A PDF that could not be read has no page to turn, zoom or scroll.
            _ => Command::Nothing,
        }
    }

    /// One named action while Quick Look holds the keyboard.
    pub fn action(&mut self, action: &str, cx: &mut Context<Self>) -> Command {
        // A refused read draws the calm card, which takes the keys of a card with no document.
        let calm = self.calm_body(cx);
        if calm.is_none() {
            if let Some(doc) = self.subject.as_ref().and_then(|s| s.body.doc()).cloned() {
                if let Some(command) = self.doc_action(&doc, action, cx) {
                    return command;
                }
            }
        }
        if let Some(walk) = self.strip() {
            return self.strip_action(&walk, action, cx);
        }
        if self.picture_action(action, cx) {
            return Command::Nothing;
        }
        let subject = self.subject.as_ref();
        let body = calm.as_ref().or(subject.map(|s| &s.body));
        let has_bar = body.is_some_and(|b| b.has_bar());
        let doc = body.and_then(|b| b.doc()).cloned();
        let surface = Surface {
            html_rendered: doc.as_ref().is_some_and(|d| d.html_rendered(cx)),
            is_document: doc.as_ref().is_some_and(|d| d.kind() == "text"),
            is_media: subject.is_some_and(|s| matches!(s.state, State::Video | State::Audio)),
            is_pdf: subject.is_some_and(|s| s.state == State::Pdf),
            is_compare: subject.is_some_and(|s| s.body.is_compare()),
            has_bar,
            close_focused: self.focused == Some(Hit::Close),
        };
        let open_with = body.is_some_and(|b| b.offers_open_with());
        let command = keys::act(action, surface);
        if let (Command::ScrollDocument(rows), Some(doc)) = (command, doc) {
            doc.scroll(rows.into(), cx);
        }
        match command {
            Command::Close => self.close(cx),
            Command::Follow(step) => cx.emit(Event::Follow(step)),
            Command::ToggleExpand => {
                self.expanded = !self.expanded;
                cx.notify();
            }
            Command::CopyPatch => cx.emit(Event::CopyPatch),
            // Compare's Tab walk: Copy Patch, then Close, round again; a refusal has Close alone.
            Command::CompareFocus(back) => {
                let walk: &[Hit] = if subject.is_some_and(|s| matches!(s.body, Body::Diff { .. })) {
                    &[Hit::CopyPatch, Hit::Close]
                } else {
                    &[Hit::Close]
                };
                let at = self.focused.and_then(|f| walk.iter().position(|w| *w == f));
                let next = match (at, back) {
                    (None, false) => 0,
                    (None, true) => walk.len() - 1,
                    (Some(i), false) => (i + 1) % walk.len(),
                    (Some(i), true) => (i + walk.len() - 1) % walk.len(),
                };
                self.focused = Some(walk[next]);
                cx.notify();
            }
            Command::Nothing if action == "open" && self.focused.is_some() => {
                if let Some(hit) = self.focused {
                    self.activate(hit, cx);
                }
            }
            Command::ToggleCloseFocus => {
                self.focused = (self.focused != Some(Hit::Close)).then_some(Hit::Close);
                cx.notify();
            }
            // With no bar, Tab walks to the card's one action, the DialogButton focus a framed button takes.
            Command::Nothing if open_with && !has_bar && action.starts_with("focus") => {
                self.focused = (self.focused != Some(Hit::OpenWith)).then_some(Hit::OpenWith);
                cx.notify();
            }
            _ => {}
        }
        command
    }
}
