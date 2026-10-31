//! The unlock card's window-side owner: runs the card's children off the UI thread and feeds their answers back.
use super::card::{CardText, Press, BOARD_BODY};
use super::host::{Effect, Host};
use super::ink::CardInk;
use super::keys::{act, Act, Key};
use super::process::{Event, Outcome, Runner, Slot};
use super::seams::RailView;
use super::unlock::{Effect as CardEffect, Proc};
use crate::list::Palette;
use flea::i18n::I18n;
use gpui::{div, prelude::*, Context, EventEmitter, FocusHandle, KeyDownEvent, Task, Window};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The window effects the host raises, for the shell to carry out.
pub struct HostEvent(pub Effect);

pub struct UnlockView {
    pub host: Host,
    pub(super) drawn: std::cell::RefCell<crate::g2::op1::editor::Drawn>,
    pub(super) input_active: bool,
    pub(super) input_palette: Option<crate::g2::op1::palette::CardPalette>,
    pub rail: Option<RailView>,
    /// What the card last painted and the pointer holds, for unlockState.
    pub probes: super::probes::Probes,
    catalog: Arc<I18n>,
    text: CardText,
    runner: Rc<dyn Runner>,
    handles: [Slot; 3],
    name_job: Slot,
    epoch: Instant,
    focus: FocusHandle,
    _readers: Vec<Task<()>>,
    _timer: Option<Task<()>>,
    _corner: gpui::Subscription,
}

impl EventEmitter<HostEvent> for UnlockView {}

#[path = "view_reading.rs"]
mod reading;

impl UnlockView {
    pub fn new(catalog: Arc<I18n>, runner: Rc<dyn Runner>, cx: &mut Context<Self>) -> Self {
        Self {
            host: Host::default(),
            drawn: Default::default(),
            input_active: false,
            input_palette: None,
            rail: None,
            probes: Default::default(),
            catalog,
            text: CardText::default(),
            runner,
            handles: Default::default(),
            name_job: Slot::default(),
            epoch: Instant::now(),
            focus: cx.focus_handle(),
            _readers: Vec::new(),
            _timer: None,
            _corner: cx.observe_global::<crate::compositor::Corner>(|_, cx| cx.notify()),
        }
    }

    pub fn now(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// A language change rebuilds the words; nothing translates during paint.
    pub fn set_catalog(&mut self, catalog: Arc<I18n>, cx: &mut Context<Self>) {
        self.catalog = catalog;
        self.refresh(cx);
    }

    pub fn open_for(
        &mut self,
        device: &str,
        label: &str,
        size: u64,
        fstype: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A new card reads no geometry until it paints, never the last card's.
        self.probes.clear();
        self.host
            .open_for(device, label, size, fstype, self.rail.as_ref());
        window.focus(&self.focus, cx);
        self.refresh(cx);
    }

    pub fn open_for_device(&mut self, device: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.probes.clear();
        self.host.open_for_device(device, self.rail.as_ref());
        window.focus(&self.focus, cx);
        self.apply(Vec::new(), cx);
    }

    /// The rail rebuilt: settle the follow and name a pending card.
    pub fn rail_changed(&mut self, rail: Option<RailView>, cx: &mut Context<Self>) {
        self.rail = rail;
        let effects = self.host.rail_rebuilt(self.rail.as_ref());
        self.apply(effects, cx);
    }

    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.sync_input();
        if let Some(card) = &self.host.card {
            self.text = CardText::of(card, &self.catalog);
        }
        cx.notify();
    }

    fn card_did(&mut self, effects: Vec<CardEffect>, cx: &mut Context<Self>) {
        let effects = self.host.card_effects(effects, self.rail.as_ref());
        self.apply(effects, cx);
    }

    fn apply(&mut self, effects: Vec<Effect>, cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                Effect::Card(CardEffect::Start(proc, job)) => {
                    let handle = self.runner.start(job);
                    let events = handle.events.clone();
                    let run = self.handles[proc as usize].start(handle);
                    self._readers.retain(|task| !task.is_ready());
                    self._readers.push(cx.spawn(async move |this, cx| {
                        while let Ok(event) = events.recv().await {
                            let done = matches!(event, Event::Exited(_));
                            if this
                                .update(cx, |view, cx| view.child_event(proc, run, event, cx))
                                .is_err()
                                || done
                            {
                                break;
                            }
                        }
                    }));
                }
                Effect::Card(CardEffect::Kill(proc)) => {
                    if let Some(handle) = self.handles[proc as usize].handle() {
                        handle.kill();
                    }
                }
                Effect::Card(CardEffect::WithholdInput(proc)) => {
                    if let Some(handle) = self.handles[proc as usize].handle() {
                        handle.withhold_input();
                    }
                }
                Effect::NameList(job) => {
                    let handle = self.runner.start(job);
                    let events = handle.events.clone();
                    let run = self.name_job.start(handle);
                    self._readers.retain(|task| !task.is_ready());
                    self._readers.push(cx.spawn(async move |this, cx| {
                        while let Ok(event) = events.recv().await {
                            if let Event::Exited(outcome) = event {
                                let _ =
                                    this.update(cx, |view, cx| view.name_exited(run, &outcome, cx));
                                break;
                            }
                        }
                    }));
                }
                Effect::NameKill => {
                    self.name_job.kill();
                }
                Effect::NameBound => {
                    self.name_job.stop();
                }
                other => cx.emit(HostEvent(other)),
            }
        }
        let now = self.now();
        let named = self.host.name_tick(now);
        if !named.is_empty() {
            return self.apply(named, cx);
        }
        self.arm_timer(cx);
        self.refresh(cx);
    }

    fn name_exited(&mut self, run: u64, outcome: &Outcome, cx: &mut Context<Self>) {
        // A killed or replaced lsblk's late exit is not the newer poller's answer.
        if !self.name_job.ended(run) {
            return;
        }
        let effects = self
            .host
            .name_exited(outcome, self.now(), self.rail.as_ref());
        self.apply(effects, cx);
    }

    fn child_event(&mut self, proc: Proc, run: u64, event: Event, cx: &mut Context<Self>) {
        let now = self.now();
        let effects = match event {
            Event::Started => self
                .host
                .card
                .as_mut()
                .map(|c| c.started(proc))
                .unwrap_or_default(),
            Event::Exited(outcome) => {
                self.handles[proc as usize].ended(run);
                return self.exited(proc, &outcome, now, cx);
            }
        };
        self.card_did(effects, cx);
    }

    fn exited(&mut self, proc: Proc, outcome: &Outcome, now: u64, cx: &mut Context<Self>) {
        let effects = self.host.exited(proc, outcome, now, self.rail.as_ref());
        self.apply(effects, cx);
    }

    /// One timer at the next deadline of the card or its name poller, never a per-frame poll.
    fn arm_timer(&mut self, cx: &mut Context<Self>) {
        let card = self.host.card.as_ref().and_then(|c| c.deadline());
        let Some(at) = [card, self.host.name_deadline()]
            .into_iter()
            .flatten()
            .min()
        else {
            self._timer = None;
            return;
        };
        let wait = Duration::from_millis(at.saturating_sub(self.now()));
        self._timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |view, cx| {
                let now = view.now();
                let effects = view
                    .host
                    .card
                    .as_mut()
                    .map(|c| c.tick(now))
                    .unwrap_or_default();
                view.card_did(effects, cx);
            });
        }));
    }

    pub fn submit(&mut self, cx: &mut Context<Self>) {
        let now = self.now();
        let effects = self
            .host
            .card
            .as_mut()
            .map(|c| c.submit(now))
            .unwrap_or_default();
        self.card_did(effects, cx);
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        let effects = self
            .host
            .card
            .as_mut()
            .map(|c| c.close())
            .unwrap_or_default();
        self.card_did(effects, cx);
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.private_key(event, window, cx) {
            return;
        }
        let Some(card) = self.host.card.as_mut().filter(|c| c.opened) else {
            return;
        };
        let stroke = &event.keystroke;
        let plain =
            !stroke.modifiers.control && !stroke.modifiers.alt && !stroke.modifiers.platform;
        let key = Key {
            name: &stroke.key,
            text: stroke.key_char.as_deref(),
            shift: stroke.modifiers.shift,
            plain,
            held: event.is_held,
        };
        let action = act(card, key);
        if action == Act::Ignore {
            return;
        }
        cx.stop_propagation();
        match action {
            Act::Close => return self.close(cx),
            Act::Submit => return self.submit(cx),
            Act::Tab { back } => card.step_tab(back),
            Act::Backspace => card.backspace(),
            Act::Type(text) => card.type_text(&text),
            Act::Ignore => {}
        }
        self.apply(Vec::new(), cx);
    }
}

impl Render for UnlockView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div()
            .id("g2-pl1-unlock")
            .absolute()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key));
        let Some(state) = self.host.card.as_ref().filter(|c| c.opened) else {
            self.probes.clear();
            return root;
        };
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let mut ink = CardInk::inherited(&Palette::inherited(&window.text_style(), theme));
        ink.radius = crate::compositor::corner(cx);
        let width = f32::from(window.viewport_size().width);
        let scale = f32::from(crate::list::Typeface::for_window(window, cx).body) / BOARD_BODY;
        self.input_palette = Some(crate::g2::op1::palette::CardPalette::from_list(
            &Palette::inherited(&window.text_style(), theme),
            ink.radius,
        ));
        let input = (state.phase != super::unlock::Phase::Unlocking)
            .then(|| (cx.entity(), crate::list::Typeface::for_window(window, cx)));
        let cancel = cx.listener(|view: &mut Self, _, _, cx| view.close(cx));
        let unlock = cx.listener(|view: &mut Self, _, _, cx| view.submit(cx));
        let press = Press {
            cancel: Box::new(cancel),
            unlock: Box::new(unlock),
        };
        // The board's scrim: the window under the card at half the ground, its pointer held by the card.
        root.occlude()
            .bg(ink.ground.opacity(flea::ui_logic_op1::card::scrim_alpha(0)))
            .flex()
            .items_center()
            .justify_center()
            .child(super::card::card_with_input(
                state,
                &self.text,
                &ink,
                width,
                scale,
                press,
                Some(super::card::Mounted {
                    input,
                    probes: &self.probes,
                }),
            ))
    }
}
