//! The operations overlay as one GPUI entity: it takes keys while a card is open and draws over the window.
use super::model::Operations;
use super::palette::CardPalette;
use super::press::Press;
use super::view::{self, Pointer, Scene};
use crate::pane::swap::Clock;
use flea::i18n::I18n;
use flea::ui_logic_op1::{
    card,
    confirm::Source,
    trash::{Review, Scope},
};
use gpui::{
    div, prelude::*, px, App, Context, Div, KeyDownEvent, MouseButton, Stateful, Task, Window,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "host_input.rs"]
mod input;

pub struct Overlay {
    pub(super) editor: super::editor::Editor,
    pub frame: super::measured::Frame,
    pub feedback_paint: crate::g2::fb1::observe::Paint,
    pub owner: Option<usize>,
    pub model: Operations,
    pub palette: CardPalette,
    pub catalog: Arc<I18n>,
    // Seam: a card already drawn under this one, such as Settings, so the window dims once.
    pub cards_below: usize,
    pub pointer: Pointer,
    pub(super) pressed_review: Option<u64>,
    // A capture pins every running spiral to one still phase; None lets them crawl.
    pub pinned: Option<f32>,
    pub(super) progress: super::progress::Presentation,
    epoch: Clock,
    beat: Option<(u64, Task<()>)>,
}

impl Overlay {
    pub fn new(catalog: Arc<I18n>, palette: CardPalette, preset: &str) -> Self {
        Self {
            editor: Default::default(),
            frame: Default::default(),
            feedback_paint: Default::default(),
            owner: None,
            model: Operations::new(preset),
            palette,
            catalog,
            cards_below: 0,
            pointer: Pointer::default(),
            pressed_review: None,
            pinned: None,
            progress: Default::default(),
            epoch: Clock::default(),
            beat: None,
        }
    }

    pub fn now(&self, cx: &App) -> u64 {
        self.epoch.elapsed(cx.background_executor().now())
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
        let now = self.now(cx);
        self.progress.observe(&self.model, now);
        let Some(due) = self
            .model
            .due()
            .into_iter()
            .chain(self.progress.due(now))
            .min()
        else {
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
                this.model.tick(&catalog, now);
                cx.notify();
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
        self.editor.restart();
        cx.notify();
        self.schedule(cx);
    }

    // Seam: the folder's background New File row, with the menu snapshot id its request carries.
    pub fn new_file(&mut self, id: u64, folder: &str, suggestion: Option<&str>, cx: &mut Context<Self>) {
        self.model.new_file(&self.catalog, id, folder, suggestion);
        self.editor.restart();
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
        let now = self.now(cx);
        self.progress.toggled(&self.model, now);
        self.schedule(cx);
        cx.notify();
    }

    // The destination handles editing first; the card swallows other modified keys, Escape excepted.
    pub fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch_key(event, window, cx);
    }
}

impl Overlay {
    // A button takes hover, holds its press while the left button is down, and activates on a release over it.
    fn wire(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(Press, Stateful<Div>) -> Stateful<Div> + use<> {
        let entity = cx.entity().downgrade();
        let paint = self.feedback_paint.clone();
        let observed = self.model.blocks.first().map(|(_, block)| block.id);
        let cancel_label: gpui::SharedString = self.catalog.tr("jobs.button.cancel").into();
        let dismiss_label: gpui::SharedString = self.catalog.tr("jobs.button.dismiss").into();
        move |press: Press, button: Stateful<Div>| {
            use flea::ui_logic_op1::jobs::Action;
            let button = match press {
                Press::Job(id, Action::Cancel) if Some(id) == observed => {
                    paint.watch(button, "cancel", cancel_label.clone())
                }
                Press::Job(id, Action::Dismiss) if Some(id) == observed => {
                    paint.watch(button, "dismiss", dismiss_label.clone())
                }
                _ => button,
            };
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
                        this.pressed_review = this.drawn_review();
                        this.pointer.pressed = Some(press);
                        this.pointer.motion = Some((press, true, at));
                        cx.notify();
                    });
                })
                .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                    let at = cx.background_executor().now();
                    let _ = up.update(cx, |this, cx| this.release(press, at, cx));
                })
                .on_click(move |_, _, cx| {
                    let at = cx.background_executor().now();
                    let _ = click.update(cx, |this, cx| {
                        this.release(press, at, cx);
                        if matches!(press, Press::Delete(..)) && !this.review_pointer_matches() {
                            return;
                        }
                        if !matches!(press, Press::Job(..))
                            && this.progress.dialog.holding(this.now(cx))
                        {
                            if press == Press::Delete(flea::ui_logic_op1::trash::Button::Cancel) {
                                this.cancel_held(cx);
                            }
                            return;
                        }
                        let catalog = this.catalog.clone();
                        this.model.press(&catalog, press);
                        if !matches!(press, Press::Job(..)) && !this.model.open() {
                            this.pointer = Pointer::default();
                        }
                        this.schedule(cx);
                        cx.notify();
                    });
                })
        }
    }

    // A press outside the card answers as Escape does, so a collision is cancelled on the wire too.
    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.progress.dialog.holding(self.now(cx)) {
            self.cancel_held(cx);
            return;
        }
        let catalog = self.catalog.clone();
        self.model.key(&catalog, "escape", false);
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

impl Overlay {
    pub(crate) fn sync_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_editor(window, cx);
    }
    pub(crate) fn focus_handle(&self) -> Option<gpui::FocusHandle> {
        self.editor.focus.clone()
    }
    pub(crate) fn release_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.release_mount_focus(window, cx);
    }
}

impl Render for Overlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.frame.clear();
        if let Some(theme) = cx.try_global::<crate::palette::Theme>() {
            self.palette =
                CardPalette::from_theme(&window.text_style(), *theme, self.palette.radius);
        }
        let typeface = crate::list::Typeface::for_window(window, cx);
        self.feedback_paint.clear();
        let size = window.viewport_size();
        let window_size = (f32::from(size.width), f32::from(size.height));
        let mut root = div()
            .size_full()
            .relative()
            .on_key_down(cx.listener(Self::key));
        let now = self.now(cx);
        self.progress.observe(&self.model, now);
        self.progress.commit(now);
        self.sync_editor(window, cx);
        if let Some(focus) = &self.editor.focus {
            root = root.track_focus(focus);
        }
        self.schedule(cx);
        super::progress::blocks(
            &mut self.model.blocks,
            &mut self.model.blocks_built,
            &self.progress.jobs,
            &self.catalog,
        );
        let wire = self.wire(cx);
        let pinned = self.pinned.or(cx.reduce_motion().then_some(0.));
        let scene = Scene {
            copy_field: Some(cx.entity()),
            frame: Some(&self.frame),
            typeface: &typeface,
            palette: &self.palette,
            catalog: &self.catalog,
            pointer: &self.pointer,
            wire: &wire,
            pinned,
            feedback: self
                .model
                .blocks
                .first()
                .map(|(_, block)| (block.id, &self.feedback_paint)),
        };
        if let Some(card) =
            view::jobs_snapshot(&self.progress.jobs, &self.model.blocks, &scene, window_size)
        {
            let card = card
                .occlude()
                .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation());
            let card = self
                .feedback_paint
                .watch(card.relative(), "frame", Default::default());
            root = root.child(
                div()
                    .absolute()
                    .right(px(card::JOBS_RIGHT))
                    .bottom(px(card::JOBS_BOTTOM))
                    .child(card),
            );
        }
        if let Some(card) = view::dialog_snapshot(
            self.progress.dialog.shown().as_ref(),
            self.model.emptying.running,
            &scene,
            window_size,
        ) {
            // A press on the card is the card's; a press on either button anywhere else dismisses, as Cancel.
            let card = card
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation());
            let scrim = view::scrim(&self.palette, self.cards_below, card)
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.dismiss(cx)),
                )
                .on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(|this, _, _, cx| this.dismiss(cx)),
                )
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation());
            root = root.child(scrim);
        }
        #[cfg(test)]
        let root = root.children(crate::theme_gate::frame::card_probe(self.palette, cx));
        root
    }
}
