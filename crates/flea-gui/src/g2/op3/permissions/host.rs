//! The live Permissions entity reuses the board drawing and routes every control through its model.
use super::{Permissions, Phase, Stop};
use crate::{
    g2::{
        op1::{editor::Drawn, palette::CardPalette},
        op3::{
            keys::{key_of, Key},
            shell::{Card, Control, Interactive, Mount},
            Effect,
        },
    },
    list::{Palette, Typeface},
    palette::Theme,
    pane::swap::{Clock, Progress},
};
use flea::{i18n::I18n, jsondoc::Json};
use gpui::{
    div, prelude::*, App, Context, EventEmitter, FocusHandle, KeyDownEvent, MouseButton, Task,
    Window,
};
use std::{cell::RefCell, sync::Arc, time::Duration};

mod editor;
mod ime;

pub struct Changed(pub Effect);

pub struct View {
    pub model: Permissions,
    pub catalog: Arc<I18n>,
    pub corner: f32,
    pub opened: bool,
    pub(crate) probe: crate::ipc::card_geometry::CardGeometry,
    pub(crate) painted: Option<Card>,
    focus: FocusHandle,
    editor: crate::path::entry::Entry,
    drawn: RefCell<Drawn>,
    palette: CardPalette,
    clock: Clock,
    progress: Progress<Option<Card>>,
    pending: Vec<Effect>,
    beat: Option<(u64, Task<()>)>,
}

impl EventEmitter<Changed> for View {}

impl View {
    pub fn new(
        model: Permissions,
        catalog: Arc<I18n>,
        corner: f32,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            model,
            catalog,
            corner,
            opened: true,
            probe: Default::default(),
            painted: None,
            focus: cx.focus_handle(),
            editor: crate::path::entry::Entry::default(),
            drawn: RefCell::default(),
            // Every render replaces it before the octal field can mount.
            palette: CardPalette::from_list(
                &Palette::inherited(&gpui::TextStyle::default(), Theme::default()),
                corner,
            ),
            clock: Clock::default(),
            progress: Progress::default(),
            pending: Vec::new(),
            beat: None,
        };
        view.observe(cx);
        view
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
    }

    fn flush(&mut self, effects: Vec<Effect>, cx: &mut Context<Self>) {
        let closing = effects.contains(&Effect::Close);
        if closing {
            self.progress.observe(&None, false, self.now(cx));
            if self.progress.holding(self.now(cx)) {
                self.pending = effects;
                self.schedule(cx);
                cx.notify();
                return;
            }
        }
        for effect in effects {
            if effect == Effect::Close {
                self.opened = false;
            }
            cx.emit(Changed(effect));
        }
        self.observe(cx);
        cx.notify();
    }

    fn now(&self, cx: &App) -> u64 {
        self.clock.elapsed(cx.background_executor().now())
    }

    fn observe(&mut self, cx: &mut Context<Self>) {
        let target =
            (self.opened && self.pending.is_empty()).then(|| self.model.card(&self.catalog));
        let active =
            matches!(self.model.phase, Phase::Reading | Phase::Applying) && self.pending.is_empty();
        self.progress.observe(&target, active, self.now(cx));
        self.schedule(cx);
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        let Some(due) = self.progress.due(self.now(cx)) else {
            self.beat = None;
            return;
        };
        if self.beat.as_ref().is_some_and(|(at, _)| *at == due) {
            return;
        }
        let delay = Duration::from_millis(due.saturating_sub(self.now(cx)));
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |view, cx| {
                view.beat = None;
                let effects = std::mem::take(&mut view.pending);
                view.flush(effects, cx);
            });
        });
        self.beat = Some((due, task));
    }

    pub fn reply(&mut self, reply: &Json, cx: &mut Context<Self>) {
        let effects = self.model.reply(&self.catalog, reply);
        self.flush(effects, cx);
    }

    pub fn stopped(&mut self, cx: &mut Context<Self>) {
        self.progress.supersede();
        if !self.pending.is_empty() {
            let effects = std::mem::take(&mut self.pending);
            self.flush(effects, cx);
            return;
        }
        self.model.stopped();
        self.flush(Vec::new(), cx);
    }

    pub fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.opened || !self.pending.is_empty() {
            cx.stop_propagation();
            return;
        }
        let stroke = &event.keystroke;
        if self.progress.holding(self.now(cx)) && stroke.key != "escape" {
            cx.stop_propagation();
            return;
        }
        if self.edit_octal(event, window, cx) {
            return;
        }
        if let Some(key) = key_of(event).map(|key| key.read(window.layout_mirrored())) {
            let effects = self.model.key(key);
            if stroke.key == "escape" && effects.contains(&Effect::Close) {
                self.progress.supersede();
            }
            self.flush(effects, cx);
        }
        cx.stop_propagation();
    }

    pub fn activate(&mut self, control: Control, cx: &mut Context<Self>) {
        if !self.opened || !self.pending.is_empty() {
            return;
        }
        if self.progress.holding(self.now(cx)) && control != Control::Button(0) {
            return;
        }
        let effects = match control {
            Control::Check(at)
                if at < 9 && self.model.phase == Phase::Ready && self.model.changeable() =>
            {
                self.model.focus = Stop::Box(at);
                self.model.key(Key::Space)
            }
            Control::Field if self.model.phase == Phase::Ready && self.model.changeable() => {
                self.model.focus = Stop::Octal;
                Vec::new()
            }
            Control::Button(0) => self.model.key(Key::Escape),
            Control::Button(1) if self.model.can_apply() => {
                self.model.focus = Stop::Apply;
                self.model.key(Key::Enter)
            }
            _ => Vec::new(),
        };
        if control == Control::Button(0) && effects.contains(&Effect::Close) {
            self.progress.supersede();
        }
        self.flush(effects, cx);
    }
}

impl Render for View {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.opened {
            return div().into_any_element();
        }
        self.observe(cx);
        self.sync_editor();
        let now = self.now(cx);
        let snapshot = self.progress.commit(now).clone();
        self.schedule(cx);
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        let typeface = Typeface::for_window(window, cx);
        self.palette = CardPalette::from_list(&palette, self.corner);
        let (entity, face) = (cx.entity(), typeface.clone());
        let mount = move |slot: gpui::Div| {
            crate::g2::op1::field::mount(entity.clone(), slot, &face).into_any_element()
        };
        let weak = cx.entity().downgrade();
        let wire = move |control, element: gpui::Stateful<gpui::Div>| {
            let weak = weak.clone();
            element.on_click(move |_, _, cx| {
                let _ = weak.update(cx, |view, cx| view.activate(control, cx));
                cx.stop_propagation();
            })
        };
        self.painted = snapshot.clone();
        let card = snapshot.map(|snapshot| {
            Interactive {
                wire: &wire,
                surface: theme.surface,
                mount: self.editor.editing.then_some(&mount as &Mount),
            }
            .present_observed(
                &snapshot,
                &palette,
                &typeface,
                self.corner,
                f32::from(window.viewport_size().width),
                &self.probe,
            )
        });
        div()
            .id("permissions-scrim")
            .size_full()
            .occlude()
            .when(card.is_some(), |root| {
                root.bg(palette.background.opacity(0.5))
            })
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _, _, cx| view.activate(Control::Button(0), cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|view, _, _, cx| view.activate(Control::Button(0), cx)),
            )
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .children(card)
            .into_any_element()
    }
}
