//! The network card entity; the shell consumes its typed worker, persistence and focus effects.
use super::{
    dialog::{Dialog, Effect, Focus},
    drawing::Drawn,
    words::Words,
};
use crate::{
    g2::{
        mn1::{
            lines::Env,
            menu::{Face, Menu, Outcome},
        },
        op1::palette::CardPalette,
    },
    list::Typeface,
};
use flea::{
    i18n::I18n,
    ui_logic_mn1::{fit::Rect, Brand, Entry, Text},
};
use gpui::{Context, EventEmitter, FocusHandle, Window};
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub struct NetworkEvent(pub Effect);
pub struct NetworkView {
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
    pub(super) password_selection: std::ops::Range<usize>,
    pub(super) password_reversed: bool,
    pub(super) password_marked: Option<std::ops::Range<usize>>,
    pub(super) menu: Rc<RefCell<Menu>>,
    menu_dirty: bool,
    pub(super) shown_status: Option<(&'static str, String)>,
    pub(super) dimensions: (f32, f32),
}

impl EventEmitter<NetworkEvent> for NetworkView {}
impl NetworkView {
    pub fn new(catalog: Arc<I18n>, palette: CardPalette, cx: &mut Context<Self>) -> Self {
        Self {
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
            password_selection: 0..0,
            password_reversed: false,
            password_marked: None,
            menu: Rc::default(),
            menu_dirty: true,
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
    pub fn line(&mut self, line: &str, cx: &mut Context<Self>) -> bool {
        let changed = self.model.discovery.line(line);
        if changed {
            self.changed(cx);
        }
        changed
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
        self.menu_dirty = true;
        cx.notify();
    }
    /// Observation only: whether the window's keyboard focus sits inside the card.
    pub fn holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.focus.contains_focused(window, cx)
    }
    /// Observation only: the height the body scrolls over, title and buttons included.
    pub fn scroll_content(&self) -> f32 {
        self.content_height()
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
        self.input = Default::default();
        self.password_selection = 0..0;
        self.password_reversed = false;
        self.password_marked = None;
        self.menu.borrow_mut().close();
        self.effects(effects, cx);
    }
    pub fn load_input(&mut self) {
        self.fit_focus();
        self.password_marked = None;
        let units = super::secret_edit::units(&self.model.password);
        self.password_selection = units..units;
        self.password_reversed = false;
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
        self.input = Default::default();
        self.input.editing = true;
        self.input.text = text.clone();
        self.input.move_to(text.len());
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
    pub(super) fn env<'a>(&'a self, typeface: &'a Typeface) -> Env<'a> {
        Env {
            catalog: &self.catalog,
            preset: "default",
            key_hints: false,
            typeface,
            inset: 0.,
        }
    }
    pub(super) fn sync_menu(&mut self, window: &Window) {
        if !self.menu_dirty {
            return;
        }
        self.menu_dirty = false;
        if self.model.focus != Focus::Host || self.model.busy() {
            self.menu.borrow_mut().close();
            return;
        }
        let entries = self
            .model
            .discovery
            .filtered(&self.model.form.host)
            .into_iter()
            .map(|index| {
                let host = &self.model.discovery.rows[index];
                let mut row =
                    Entry::row(index.to_string(), Text::Data(host.name.clone()), "server");
                if host.tailnet {
                    row.brand = Some(Brand::Tailscale);
                }
                let protocol = self.catalog.tr(match host.address.scheme.as_str() {
                    "sftp" => "network.sftp",
                    "ftp" | "ftps" => "network.ftps",
                    "dav" | "davs" => "network.webdav",
                    "nfs" => "network.nfs",
                    _ => "network.smb",
                });
                let tailnet = self.catalog.tr("network.tailscale");
                let caption = self.catalog.tr_args(
                    "network.host_caption",
                    &[
                        &protocol,
                        if host.tailnet {
                            &tailnet
                        } else {
                            &host.address.host
                        },
                    ],
                );
                row.hint = Some(Text::Data(caption));
                row
            })
            .collect::<Vec<_>>();
        let width = self.dimensions.0;
        let height = self.dimensions.1;
        let x = (width - super::drawing::WIDTH.min(width - 16.)) / 2. + 17.;
        let y = (height - self.frame_height(height)) / 2. + 224. - self.scroll;
        let mut typeface = Typeface::omarchy_default();
        typeface.font = window.text_style().font();
        self.menu.borrow_mut().open(
            Face::Place,
            entries,
            (x, y),
            Rect {
                x: 0.,
                y: 0.,
                w: width,
                h: height,
            },
            "",
            &self.env(&typeface),
            window,
        );
        if let Some(card) = &mut self.menu.borrow_mut().card {
            card.frame.w = super::drawing::WIDTH.min(width - 16.) - 34.;
            for line in &mut card.built.lines {
                line.key = line.word.take();
            }
        }
    }
    pub(super) fn menu_outcome(&mut self, outcome: Outcome, cx: &mut Context<Self>) -> bool {
        if let Outcome::Fire { action, .. } = outcome {
            if let Ok(index) = action.parse() {
                self.model.choose_suggestion(index);
                self.load_input();
                self.changed(cx);
            }
            return true;
        }
        false
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
