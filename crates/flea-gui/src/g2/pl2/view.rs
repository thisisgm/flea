//! The network card entity; the shell consumes its typed worker, persistence and focus effects.
use super::{
    dialog::{Dialog, Effect, Focus},
    drawing::Drawn,
    words::Words,
};
use crate::g2::op1::palette::CardPalette;
use flea::i18n::I18n;
use gpui::{Context, EventEmitter, FocusHandle, Window};
use std::{cell::RefCell, sync::Arc};

pub struct NetworkEvent(pub Effect);
pub struct NetworkView {
    pub(super) motion: crate::reveal::Reveal,
    pub model: Dialog,
    pub palette: CardPalette,
    pub cards_below: usize,
    pub(crate) catalog: Arc<I18n>,
    pub(crate) words: Words,
    pub(crate) drawn: RefCell<Drawn>,
    pub(crate) revision: u64,
    pub hovered: Option<Focus>,
    pub pressed: Option<Focus>,
    pub(super) selecting: Option<usize>,
    pub scroll: f32,
    pub(super) focus: FocusHandle,
    pub input: crate::path::entry::Entry,
    pub(super) input_owner: Option<Focus>,
    pub(super) input_origin: std::cell::Cell<gpui::Point<gpui::Pixels>>,
    pub(super) password_selection: std::ops::Range<usize>,
    pub(super) password_reversed: bool,
    pub(super) password_marked: Option<std::ops::Range<usize>>,
    pub(super) shown_status: Option<(&'static str, String)>,
    pub(super) dimensions: (f32, f32),
}

impl EventEmitter<NetworkEvent> for NetworkView {}
impl NetworkView {
    pub fn new(catalog: Arc<I18n>, palette: CardPalette, cx: &mut Context<Self>) -> Self {
        Self {
            motion: Default::default(),
            words: Words::new(&catalog),
            catalog,
            palette,
            cards_below: 0,
            model: Dialog::default(),
            drawn: RefCell::default(),
            revision: 1,
            hovered: None,
            pressed: None,
            selecting: None,
            scroll: 0.,
            focus: cx.focus_handle(),
            input: Default::default(),
            input_owner: None,
            input_origin: Default::default(),
            password_selection: 0..0,
            password_reversed: false,
            password_marked: None,
            shown_status: None,
            dimensions: (0., 0.),
        }
    }
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.opened || self.model.busy() {
            return;
        }
        let effects = self.model.open();
        self.scroll = 0.;
        self.load_input();
        self.effects(effects, cx);
        window.focus(&self.focus, cx);
    }
    /// Add a Network Share opened from a nearby host: the card at rest, then that host's fields filled.
    pub fn open_suggested(
        &mut self,
        origin: u64,
        address: &flea::ui_logic_pl2::networkaddress::Address,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_for(origin, window, cx);
        self.model.suggest(address);
        self.load_input();
        self.changed(cx);
    }
    pub(crate) fn visible(&self) -> bool {
        self.model.opened || self.motion.visible()
    }
    pub fn open_for(&mut self, origin: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.opened || self.model.busy() {
            return;
        }
        self.model.origin = origin;
        self.open(window, cx);
    }
    pub fn mount_finished(
        &mut self,
        id: u64,
        uri: &str,
        result: Result<(), &'static str>,
        connect_failed: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.model.opened
            || !self.model.connecting
            || self.model.request != id
            || self.model.pending_uri != uri
        {
            return;
        }
        let effects = self.model.mount_finished(id, uri, result, connect_failed);
        self.load_input();
        self.effects(effects, cx);
    }
    pub fn saved(&mut self, id: u64, success: bool, detail: &str, cx: &mut Context<Self>) {
        if !self.model.opened || !self.model.saving || self.model.request != id {
            return;
        }
        let effects = self.model.saved(id, success, detail);
        self.load_input();
        self.effects(effects, cx);
    }
    pub fn backend_died(&mut self, cx: &mut Context<Self>) {
        let effects = self.model.backend_died();
        self.load_input();
        self.effects(effects, cx);
    }
    pub fn set_catalog(&mut self, catalog: Arc<I18n>, cx: &mut Context<Self>) {
        self.words = Words::new(&catalog);
        self.catalog = catalog;
        self.changed(cx);
    }
    pub fn set_palette(&mut self, palette: CardPalette, cx: &mut Context<Self>) {
        self.palette = palette;
        self.changed(cx);
    }
    pub fn effects(&mut self, effects: Vec<Effect>, cx: &mut Context<Self>) {
        for effect in effects {
            cx.emit(NetworkEvent(effect));
        }
        self.changed(cx);
    }
    pub fn changed(&mut self, cx: &mut Context<Self>) {
        let status = self
            .model
            .status
            .map(|key| (key, self.model.detail.clone()));
        if status != self.shown_status {
            self.show_status();
            self.shown_status = status;
        }
        self.revision += 1;
        cx.notify();
    }
    pub(super) fn paint_changed(&mut self, cx: &mut Context<Self>) {
        self.revision += 1;
        cx.notify();
    }

    pub fn reveal_held(&mut self, held: bool, cx: &mut Context<Self>) {
        self.model.reveal = held
            && self.model.opened
            && !self.model.busy()
            && self.model.form.protocol.credentials();
        self.paint_changed(cx);
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.selecting = None;
        let effects = self.model.close();
        if self.model.opened {
            return;
        }
        self.input.close();
        self.input_owner = None;
        self.drawn.borrow_mut().ime.registered = None;
        self.password_selection = 0..0;
        self.password_reversed = false;
        self.password_marked = None;
        self.effects(effects, cx);
    }
    pub fn load_input(&mut self) {
        self.fit_focus();
        self.input_owner = None;
        self.drawn.borrow_mut().ime.registered = None;
        self.password_marked = None;
        let units = super::secret_edit::units(&self.model.password);
        self.password_selection = units..units;
        self.password_reversed = false;
        if self.model.focus == Focus::Password
            && self.model.opened
            && !self.model.busy()
            && !self.model.committed
        {
            crate::g2::pl1::private_entry::begin(&mut self.input, &self.model.password);
            self.input.move_to(self.input.text.len());
            self.input_owner = Some(Focus::Password);
            return;
        }
        let text = match self.model.focus {
            Focus::Label => &self.model.form.label,
            Focus::Host => &self.model.form.host,
            Focus::Port => &self.model.form.port,
            Focus::Path => &self.model.form.path,
            Focus::Domain => &self.model.form.domain,
            Focus::User => &self.model.form.user,
            _ => {
                self.input = Default::default();
                return;
            }
        };
        self.input.kind = if self.model.focus == Focus::Port {
            crate::path::entry::Kind::Digits
        } else {
            crate::path::entry::Kind::Text
        };
        self.input.begin(text);
        self.input.move_to(text.len());
        self.input_owner = Some(self.model.focus);
    }
    pub(super) fn fit_focus(&mut self) {
        if self.dimensions.1 <= 0. {
            return;
        }
        let height = self.body_height(self.dimensions.1);
        let Some(mut rect) = super::drawing::field(self.model.focus) else {
            return;
        };
        if self.model.form.protocol != flea::ui_logic_pl2::form::Protocol::Smb
            && matches!(self.model.focus, Focus::User | Focus::Password)
        {
            rect.y -= 65.;
        }
        if rect.y - self.scroll < 61. {
            self.scroll = rect.y - 61.;
        }
        if rect.y + rect.h - self.scroll > height - 69. {
            self.scroll = rect.y + rect.h - (height - 69.);
        }
        self.scroll = self
            .scroll
            .clamp(0., (self.content_height() - height).max(0.));
    }
    /// The LABEL hint names the share from committed text, never from a HOST or SHARE preedit.
    pub(super) fn derived_name(&self) -> String {
        let mut form = std::borrow::Cow::Borrowed(&self.model.form);
        if self.input.marked.is_some() {
            let committed = self.input.committed().into_owned();
            match self.input_owner {
                Some(Focus::Host) => form.to_mut().host = committed,
                Some(Focus::Path) => form.to_mut().path = committed,
                _ => {}
            }
        }
        form.name()
    }
    pub fn edited(&mut self, cx: &mut Context<Self>) {
        let target = match self.model.focus {
            Focus::Label => &mut self.model.form.label,
            Focus::Host => &mut self.model.form.host,
            Focus::Port => &mut self.model.form.port,
            Focus::Path => &mut self.model.form.path,
            Focus::Domain => &mut self.model.form.domain,
            Focus::User => &mut self.model.form.user,
            _ => return,
        };
        target.clone_from(&self.input.text);
        self.changed(cx);
    }
    pub(super) fn pointer_activate(&mut self, focus: Focus, cx: &mut Context<Self>) {
        let Focus::Protocol(index) = focus else {
            return self.activate(focus, cx);
        };
        if self.model.busy() || self.model.committed || !self.model.ring().contains(&focus) {
            return;
        }
        let previous = self.model.focus;
        self.model.pick(index);
        if self.model.focus != previous || previous == Focus::Port {
            self.load_input();
        }
        self.changed(cx);
    }

    pub(super) fn activate(&mut self, focus: Focus, cx: &mut Context<Self>) {
        if focus == Focus::Eye {
            return;
        }
        if !self.model.ring().contains(&focus) {
            return;
        }
        if self.model.busy() && focus != Focus::Cancel {
            return;
        }
        if self.model.committed && !matches!(focus, Focus::Cancel | Focus::Connect | Focus::Dropbox)
        {
            return;
        }
        self.model.focus = focus;
        match focus {
            Focus::Protocol(index) => self.model.pick(index),
            Focus::Cancel => return self.close(cx),
            Focus::Dropbox => {
                let effects = self.model.install_dropbox();
                self.effects(effects, cx);
            }
            Focus::Connect => {
                let effects = self.model.submit();
                self.effects(effects, cx);
            }
            Focus::Eye => {}
            Focus::Tls => self.model.form.toggle_tls(),
            _ => {}
        }
        self.load_input();
        self.changed(cx);
    }
}
