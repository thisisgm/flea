//! The lazy Settings and keyboard-sheet mounts, shared settings writer, and per-window state.
mod actions;
mod apply;
mod claim;
#[cfg(test)]
mod fix1_tests;
mod locale;
mod messages;
mod plugins;
#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod service_tests;
mod store;
mod update;
pub(crate) use store::SessionStore;

use crate::{
    g2::st1::{self, Facts, Panel, Request, SettingsView, Sheet, SheetView, Sources},
    state::Shell,
};
use flea::jsondoc::Json;
use gpui::{AppContext, Context, Entity, Subscription, Task, Window};

#[derive(Default)]
pub(crate) struct Host {
    pub panel: Option<Entity<SettingsView>>,
    pub sheet: Option<Entity<SheetView>>,
    pub opened: bool,
    pub plugins: Option<Entity<crate::g2::st2::view::PluginsPane>>,
    pub plugin_queue: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    pub plugin_backends: [bool; 2],
    pub patches: store::Patches,
    pub pending: Vec<(u64, Json)>,
    pub failed: Vec<(u64, Json)>,
    // Request revision and highest failed revision actually included in that request.
    pub retried: Vec<(u64, u64)>,
    pub favourite_focus: Option<(u64, usize)>,
    pub revision: u64,
    pub claim: st1::claim::Claim,
    update: flea::ui_logic_st1::update::Status,
    updating: bool,
    pub about_open: bool,
    pub listed_paths: [String; 2],
    pub relist: [bool; 2],
    pub sources: Sources,
    pub messages: Option<Entity<crate::g2::fb1::line::MessageLine>>,
    sheet_subscription: Option<Subscription>,
    worker: Option<store::Worker>,
    reader: Option<Task<()>>,
    pub(super) subscriptions: Vec<Subscription>,
}

pub struct SheetDecision(pub flea::ui_logic_st1::sheet::Decision);
impl gpui::EventEmitter<SheetDecision> for Shell {}

impl Shell {
    pub(super) fn settings_base(&self, cx: &gpui::App) -> u32 {
        cx.try_global::<crate::presentation::Fonts>()
            .map_or(14, |fonts| fonts.base)
    }

    pub(super) fn settings_typeface(&self, cx: &gpui::App) -> crate::list::Typeface {
        cx.try_global::<crate::presentation::Fonts>()
            .cloned()
            .unwrap_or_else(|| {
                crate::presentation::Fonts::from_sources(
                    "[font]\nbase-size=14",
                    "",
                    self.active_typeface().font.family.to_string(),
                )
            })
            .typeface(&self.launch.settings.data)
    }

    pub(crate) fn settings_surface(&self) -> Option<gpui::AnyElement> {
        use gpui::{div, prelude::*};
        let view: gpui::AnyView = if let Some(view) = &self.settings.sheet {
            view.clone().into()
        } else if self.settings.opened {
            self.settings.panel.as_ref()?.clone().into()
        } else {
            return None;
        };
        Some(
            gpui::deferred(div().absolute().inset_0().child(view))
                .with_priority(2)
                .into_any_element(),
        )
    }

    pub(crate) fn follow_settings(&mut self, window: &Window, cx: &mut Context<Self>) {
        let worker = match store::Worker::start() {
            Ok(worker) => worker,
            Err(_) => return,
        };
        let updates = worker.updates.clone();
        self.settings.worker = Some(worker);
        self.settings.reader = Some(cx.spawn_in(window, async move |this, cx| {
            while let Ok(update) = updates.recv().await {
                if cx
                    .update(|window, cx| {
                        this.update(cx, |shell, cx| {
                            shell.receive_settings(update, window, cx);
                        })
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    pub(crate) fn stop_settings(&mut self) {
        self.settings.worker = None;
        self.settings.reader = None;
    }

    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.sheet = None;
        self.settings.sheet_subscription = None;
        self.settings.about_open = false;
        self.keyboard.disarm();
        let pane_path = self.panes[self.focus_side]
            .as_ref()
            .map(|p| p.path.clone())
            .unwrap_or_default();
        if self.settings.panel.is_none() {
            let facts = Facts {
                corner: self.corner,
                base_size: self.settings_base(cx),
                home: std::env::var("HOME").unwrap_or_default(),
                pane_path: pane_path.clone(),
                monitor_scale: f64::from(window.scale_factor()),
                language: self.locale.catalog.locale().name.clone(),
                ..Facts::default()
            };
            let panel = Panel::new(
                Box::new(SessionStore {
                    document: self.launch.settings.data.clone(),
                    patches: self.settings.patches.clone(),
                }),
                facts,
                self.locale.catalog.clone(),
            );
            let typeface = self.settings_typeface(cx);
            let view = cx.new(|cx| {
                let mut view = SettingsView::new(panel, cx);
                view.typeface = typeface;
                view
            });
            self.settings.subscriptions.push(cx.observe_in(
                &view,
                window,
                |shell, _, window, cx| {
                    shell.sync_settings(window, cx);
                },
            ));
            self.settings.subscriptions.push(cx.subscribe_in(
                &view,
                window,
                |shell, _, request: &Request, window, cx| {
                    shell.settings_request(request.clone(), window, cx);
                },
            ));
            self.settings.panel = Some(view);
        }
        self.settings.opened = true;
        let view = self.settings.panel.clone().expect("settings built");
        view.update(cx, |view, cx| {
            view.panel.facts.pane_path = pane_path;
            view.panel.side = st1::Side::Pane;
            view.panel.show_section(view.panel.section);
            window.focus(&view.focus, cx);
            cx.notify();
        });
        cx.notify();
    }

    pub(crate) fn open_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.opened = false;
        self.keyboard.disarm();
        let preset =
            flea::ui_logic_st1::pages::word(&self.launch.settings.data, &["keys"], "default");
        let sheet = Sheet::new(
            preset,
            self.locale.catalog.clone(),
            Sources {
                home: std::env::var("HOME").unwrap_or_default(),
                ..self.settings.sources.clone()
            },
        );
        let corner = self.corner;
        let typeface = self.settings_typeface(cx);
        let view = cx.new(|cx| {
            let mut view = SheetView::new(sheet, cx);
            view.corner = corner;
            view.typeface = typeface;
            window.focus(&view.focus, cx);
            view
        });
        self.settings.sheet_subscription =
            Some(
                cx.subscribe_in(&view, window, |shell, _, request: &Request, window, cx| {
                    shell.settings_request(request.clone(), window, cx);
                }),
            );
        self.settings.sheet = Some(view);
        cx.notify();
    }

    fn settings_request(&mut self, request: Request, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_settings(window, cx);
        match request {
            Request::Close => {
                let decision = self
                    .settings
                    .sheet
                    .as_ref()
                    .and_then(|view| view.read(cx).sheet.decided.clone());
                self.settings.opened = false;
                self.settings.about_open = false;
                self.settings.sheet = None;
                self.settings.sheet_subscription = None;
                window.focus(&self.focus, cx);
                if let Some(decision) = decision {
                    if let flea::ui_logic_st1::sheet::Decision::Action { action } = &decision {
                        self.settings_host_action(action, window, cx);
                    } else {
                        cx.emit(SheetDecision(decision));
                    }
                }
            }
            Request::OpenSheet => self.open_sheet(window, cx),
            Request::Host(id) => self.settings_host_action(&id, window, cx),
            Request::MoveFavourite { from, to } => {
                self.move_settings_favourite(from, Some(to), window, cx)
            }
            Request::RemoveFavourite(from) => self.move_settings_favourite(from, None, window, cx),
        }
        cx.notify();
    }
}

impl Host {
    pub(crate) fn row_states(&self, listing: usize, cursor: usize) -> crate::list::RowStates {
        let mut states = crate::list::RowStates::cursor_only(listing, cursor);
        states.focused = !self.opened && self.sheet.is_none();
        states
    }
}

#[cfg(test)]
mod density_prepare_tests;
