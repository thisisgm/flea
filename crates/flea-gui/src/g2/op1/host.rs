//! The operations overlay as one GPUI entity: it takes keys while a card is open and draws over the window.
use super::model::Operations;
use super::palette::CardPalette;
use super::press::Press;
use super::view::{self, Pointer, Scene};
use flea::i18n::I18n;
use flea::ui_logic_op1::{
    card,
    confirm::Source,
    jobs::State,
    trash::{Review, Scope},
};
use gpui::{
    div, prelude::*, px, App, Context, Div, FocusHandle, KeyDownEvent, MouseButton, Stateful, Task,
    Window,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct Overlay {
    pub model: Operations,
    pub palette: CardPalette,
    pub catalog: Arc<I18n>,
    // Seam: a card already drawn under this one, such as Settings, so the window dims once.
    pub cards_below: usize,
    pub pointer: Pointer,
    // A capture pins every running spiral to one still phase; None lets them crawl.
    pub pinned: Option<f32>,
    // When the Jobs card last appeared, so its spirals crawl from their first frame.
    pub since: Option<Instant>,
    // The host's own clock: milliseconds since its first line, so the beat and the check need no caller.
    epoch: Option<Instant>,
    beat: Option<(u64, Task<()>)>,
    focus: Option<FocusHandle>,
    previous_focus: Option<FocusHandle>,
}

impl Overlay {
    pub fn new(catalog: Arc<I18n>, palette: CardPalette, preset: &str) -> Self {
        Self {
            model: Operations::new(preset),
            palette,
            catalog,
            cards_below: 0,
            pointer: Pointer::default(),
            pinned: None,
            since: None,
            epoch: None,
            beat: None,
            focus: None,
            previous_focus: None,
        }
    }

    // One focus handle serves the card's internal field/button focus; no handle exists before a modal opens.
    pub(crate) fn sync_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.model.open() {
            self.release_focus(window, cx);
            return;
        }
        if self.focus.is_none() {
            self.previous_focus = window.focused(cx);
            self.focus = Some(cx.focus_handle());
        }
        window.focus(self.focus.as_ref().expect("the open card owns focus"), cx);
    }

    // The handle the open card holds focus with; None while no card is open.
    #[cfg(test)]
    pub(crate) fn focus_handle(&self) -> Option<FocusHandle> {
        self.focus.clone()
    }

    pub(crate) fn release_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(focus) = self.focus.take() else {
            return;
        };
        let previous = self.previous_focus.take();
        if focus.is_focused(window) {
            if let Some(previous) = previous {
                window.focus(&previous, cx);
            } else {
                window.blur(cx);
            }
        }
    }

    pub fn now(&mut self, cx: &App) -> u64 {
        let now = cx.background_executor().now();
        let epoch = *self.epoch.get_or_insert(now);
        now.saturating_duration_since(epoch).as_millis() as u64
    }

    // Seam: the window's transport forwards each backend line here before its own readers.
    pub fn line(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let now = self.now(cx);
        let taken = self.model.line(&self.catalog, text, now);
        if self.model.changed {
            cx.notify();
        }
        self.schedule(cx);
        taken
    }

    // One timer for the held items' beat and the open confirmation's check, re-armed whenever the earliest due moves.
    pub fn schedule(&mut self, cx: &mut Context<Self>) {
        let Some(due) = self.model.due() else {
            self.beat = None;
            return;
        };
        if self.beat.as_ref().is_some_and(|(at, _)| *at == due) {
            return;
        }
        let wait = Duration::from_millis(due.saturating_sub(self.now(cx)));
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |this, cx| {
                this.beat = None;
                let now = this.now(cx);
                let catalog = this.catalog.clone();
                if this.model.tick(&catalog, now) {
                    cx.notify();
                }
                this.schedule(cx);
            });
        });
        self.beat = Some((due, task));
    }

    // Seam: the menu's Delete permanently or the Trash view's prepare reply opens the card on the host's clock and arms its check.
    pub fn confirm_delete(
        &mut self,
        review: Review,
        scope: Scope,
        source: Source,
        cx: &mut Context<Self>,
    ) {
        let now = self.now(cx);
        self.model.confirm_delete(review, scope, source, now);
        cx.notify();
        self.schedule(cx);
    }

    // Seam: the menu's Copy To or Move To row, with the menu snapshot id it was opened from.
    pub fn copy_to(&mut self, menu: u64, moving: bool, path: &str, cx: &mut Context<Self>) {
        self.model.copy_to(menu, moving, path);
        cx.notify();
        self.schedule(cx);
    }

    // Seam: the Trash view says when its own request is out; the check waits for it, then a full interval runs again.
    pub fn trash_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        if self.model.trash_busy && !busy {
            let now = self.now(cx);
            if let Some(c) = self.model.confirm.as_mut().filter(|c| c.is_trash()) {
                c.restart(now);
            }
        }
        self.model.trash_busy = busy;
        self.schedule(cx);
    }

    // Seam: the Trash view closed; its confirmation ends unless a delete is already out.
    pub fn trash_closed(&mut self, cx: &mut Context<Self>) {
        self.model.trash_closed();
        cx.notify();
        self.schedule(cx);
    }

    // Seam: the status bar's click on the strip's count folds the Jobs card away and brings it back.
    pub fn fold_jobs(&mut self, cx: &mut Context<Self>) {
        self.model.jobs.toggle_fold();
        cx.notify();
    }

    // A card swallows Control, Alt and Super combinations whole, Escape excepted, as every v0.3.12 card does.
    pub fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_focus(window, cx);
        let keys = &event.keystroke;
        let held = keys.modifiers.control || keys.modifiers.alt || keys.modifiers.platform;
        if held && keys.key != "escape" {
            if self.model.open() {
                cx.stop_propagation();
            }
            return;
        }
        if self
            .model
            .key(&self.catalog, &keys.key, keys.modifiers.shift)
        {
            cx.stop_propagation();
            cx.notify();
            self.schedule(cx);
        }
        self.sync_focus(window, cx);
    }
}

impl Overlay {
    // A button takes hover, holds its press while the left button is down, and activates on a release over it.
    fn wire(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(Press, Stateful<Div>) -> Stateful<Div> + use<> {
        let entity = cx.entity().downgrade();
        move |press, button| {
            let (hover, down, up, click) = (
                entity.clone(),
                entity.clone(),
                entity.clone(),
                entity.clone(),
            );
            button
                .on_hover(move |hovered, _, cx| {
                    let _ = hover.update(cx, |this, cx| {
                        let now = if *hovered {
                            Some(press)
                        } else {
                            this.pointer.hovered.filter(|h| *h != press)
                        };
                        this.pointer.hovered = now;
                        cx.notify();
                    });
                })
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    let at = cx.background_executor().now();
                    let _ = down.update(cx, |this, cx| {
                        this.pointer.pressed = Some(press);
                        this.pointer.motion = Some((press, true, at));
                        cx.notify();
                    });
                })
                .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                    let at = cx.background_executor().now();
                    let _ = up.update(cx, |this, cx| this.release(press, at, cx));
                })
                .on_click(move |_, window, cx| {
                    let at = cx.background_executor().now();
                    let _ = click.update(cx, |this, cx| {
                        this.release(press, at, cx);
                        let catalog = this.catalog.clone();
                        this.model.press(&catalog, press);
                        this.sync_focus(window, cx);
                        if !matches!(press, Press::Job(..)) && !this.model.open() {
                            this.pointer = Pointer::default();
                        }
                        this.schedule(cx);
                    });
                })
        }
    }

    // A press outside the card answers as Escape does, so a collision is cancelled on the wire too.
    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let catalog = self.catalog.clone();
        self.model.key(&catalog, "escape", false);
        self.sync_focus(window, cx);
        self.pointer = Pointer::default();
        cx.notify();
        self.schedule(cx);
    }

    fn release(&mut self, press: Press, at: Instant, cx: &mut Context<Self>) {
        if self.pointer.pressed == Some(press) {
            self.pointer.pressed = None;
            self.pointer.motion = Some((press, false, at));
            cx.notify();
        }
    }
}

impl Render for Overlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_focus(window, cx);
        if let Some(theme) = cx.try_global::<crate::palette::Theme>() {
            self.palette =
                CardPalette::from_theme(&window.text_style(), *theme, self.palette.radius);
        }
        let typeface = crate::list::Typeface::for_window(window, cx);
        let size = window.viewport_size();
        let window_size = (f32::from(size.width), f32::from(size.height));
        let mut root = div()
            .size_full()
            .relative()
            .when_some(self.focus.as_ref(), |root, focus| root.track_focus(focus))
            .on_key_down(cx.listener(Self::key));
        let wire = self.wire(cx);
        let showing = !self.model.jobs.folded && self.model.jobs.visible();
        let since = match (showing, self.since) {
            (true, Some(at)) => at,
            (true, None) => *self.since.insert(cx.background_executor().now()),
            (false, _) => {
                self.since = None;
                cx.background_executor().now()
            }
        };
        self.model.refresh_blocks(&self.catalog);
        // Reduced motion holds every spiral still; otherwise one frame request moves all of them.
        let pinned = self.pinned.or(cx.reduce_motion().then_some(0.));
        if showing
            && pinned.is_none()
            && self
                .model
                .blocks
                .iter()
                .any(|(_, b)| b.state == State::Running)
        {
            window.request_animation_frame();
        }
        let scene = Scene {
            typeface: &typeface,
            palette: &self.palette,
            catalog: &self.catalog,
            pointer: &self.pointer,
            wire: &wire,
            pinned,
            since,
        };
        if let Some(card) = view::jobs_card(&self.model, &scene, window_size) {
            root = root.child(
                div()
                    .absolute()
                    .right(px(card::JOBS_RIGHT))
                    .bottom(px(card::JOBS_BOTTOM))
                    .child(card),
            );
        }
        if let Some(card) = view::dialog(&self.model, &scene, window_size) {
            // A press on the card is the card's; a press on either button anywhere else dismisses, as Cancel.
            let card = card
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation());
            let scrim = view::scrim(&self.palette, self.cards_below, card)
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.dismiss(window, cx)),
                )
                .on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(|this, _, window, cx| this.dismiss(window, cx)),
                )
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation());
            root = root.child(scrim);
        }
        root
    }
}
