//! The lazy Settings and keyboard-sheet mounts, shared settings writer, and per-window state.
mod actions;
#[cfg(test)]
mod airdrop_tests;
mod apply;
mod claim;
#[cfg(test)]
mod codeicons_tests;
#[cfg(test)]
mod corner_tests;
#[cfg(test)]
mod fix1_tests;
#[cfg(test)]
pub(crate) mod ink_tests;
#[cfg(test)]
mod keymap_ipc_tests;
mod locale;
#[cfg(test)]
mod m9_g1rows_05_tests;
mod messages;
mod network;
#[cfg(test)]
mod notice_tests;
#[cfg(test)]
mod pb_favourite_tests;
mod plugins;
mod provenance;
mod relist;
#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod service_tests;
mod sheet;
pub(crate) mod store;
#[cfg(test)]
mod textsize_tests;
#[cfg(test)]
mod today_tests;
mod update;
#[cfg(test)]
mod update_poll_tests;
pub(crate) use store::SessionStore;

use crate::{
    g2::settings_pages::{self, Facts, Panel, Request, SettingsView, SheetView},
    state::Shell,
};
use flea::jsondoc::Json;
use gpui::{AppContext, Context, Entity, Subscription, Task, Window};

// The Plugins page's lazy pane, the showing it last synced in, its queued backend requests and the two backends' readiness.
#[derive(Default)]
pub(crate) struct Plugins {
    pub pane: Option<Entity<crate::g2::plugin_settings::view::PluginsPane>>,
    pub shown: bool,
    pub queue: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    pub backends: [bool; 2],
}

#[derive(Default)]
pub(crate) struct Host {
    pub panel: Option<Entity<SettingsView>>,
    pub sheet: Option<Entity<SheetView>>,
    pub opened: bool,
    pub plugins: Plugins,
    pub patches: store::Patches,
    pub pending: Vec<(u64, Json)>,
    // Revisions of Favorites operations the writer has not answered yet.
    pub favourite_writes: Vec<u64>,
    pub failed: Vec<(u64, Json)>,
    // Request revision and highest failed revision actually included in that request.
    pub retried: Vec<(u64, u64)>,
    pub favourite_focus: Option<(u64, usize)>,
    pub network_favourite: Option<(u64, u64)>,
    pub revision: u64,
    pub claim: settings_pages::claim::Claim,
    update: flea::ui_logic_settings_pages::update::Status,
    updating: bool,
    poll: Option<Task<()>>,
    pub about_open: bool,
    // The keyboard context Settings or the key sheet opened from, so closing can return to the rail.
    pub opened_from: flea::keys::Context,
    pub listed_paths: [String; 2],
    pub relist: [bool; 2],
    /// Hidden changed, so the owed re-list is a fresh one back at row 0 (Pane.qml re-opens the folder).
    pub relist_fresh: [bool; 2],
    sheet_sources_subscription: Option<Subscription>,
    pub messages: Option<Entity<crate::g2::status_feedback::line::MessageLine>>,
    sheet_subscription: Option<Subscription>,
    worker: Option<store::Worker>,
    reader: Option<Task<()>>,
    pub(super) subscriptions: Vec<Subscription>,
}

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

    /// Seeds the media level from preview.volume; a kept level is written back as that leaf, ViewState.setMediaVolume.
    pub(crate) fn follow_media_volume(&mut self, window: &Window, cx: &mut Context<Self>) {
        use crate::g2::media_preview::sound::MediaSound;
        let shell = cx.entity().downgrade();
        let handle = window.window_handle();
        let keep: crate::g2::media_preview::sound::KeepVolume =
            std::rc::Rc::new(move |level, cx| {
                let shell = shell.clone();
                // A release lands inside a player or shell update, so the write waits for the next turn.
                cx.defer(move |cx| {
                    let Ok(patch) =
                        flea::jsondoc::parse(&flea::ui_logic_media_preview::volume_patch(level))
                    else {
                        return;
                    };
                    let _ = handle.update(cx, |_, window, cx| {
                        shell.update(cx, |shell, cx| shell.change_settings(patch, window, cx))
                    });
                });
            });
        let stored = MediaSound::stored(&self.launch.settings.data);
        MediaSound::install(stored, Some(keep), cx);
    }

    pub(crate) fn follow_settings(&mut self, window: &Window, cx: &mut Context<Self>) {
        #[cfg(test)]
        if cx.has_global::<crate::state::constructor_fixture::NoSettingsWorker>() {
            return;
        }
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

    pub(crate) fn apply_corner(
        &mut self,
        corner: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let live = crate::compositor::valid_corner(f64::from(corner));
        // The app global carries only the compositor value; a scene corner stays in this window.
        cx.set_global(crate::compositor::Corner(live));
        let corner = self.visual.scene_corner.unwrap_or(live);
        self.corner = corner;
        if let Some(view) = &self.settings.panel {
            view.update(cx, |view, cx| {
                view.panel.facts.corner = corner;
                cx.notify();
            });
        }
        if let Some(view) = &self.settings.sheet {
            view.update(cx, |view, cx| {
                view.corner = corner;
                cx.notify();
            });
        }
        if let Some(view) = &self.settings.plugins.pane {
            let theme = cx
                .try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default();
            let palette = crate::list::Palette::inherited(&window.text_style(), theme);
            view.update(cx, |view, cx| {
                view.set_tokens(
                    crate::g2::plugin_settings::seam::Tokens::on_surface(&palette, theme),
                    corner,
                    cx,
                );
            });
        }
        self.restyle_frames(window, cx);
        if let Some(view) = &self.chrome.view {
            let path = view.read(cx).path.clone();
            path.update(cx, |view, cx| {
                view.corner = corner;
                cx.notify();
            });
            view.update(cx, |view, cx| {
                view.inputs.corner = corner;
                cx.notify();
            });
        }
        if let Some(join) = &self.rail {
            join.places.update(cx, |view, cx| {
                view.model.borrow_mut().corner = corner;
                cx.notify();
            });
        }
        if let Some(host) = &self.trash {
            host.pane.update(cx, |view, cx| {
                view.paint.borrow_mut().ink.radius = gpui::px(corner);
                cx.notify();
            });
        }
        cx.notify();
    }

    pub(crate) fn stop_settings(&mut self) {
        self.settings.worker = None;
        self.settings.reader = None;
    }

    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.sheet = None;
        self.settings.sheet_subscription = None;
        self.settings.sheet_sources_subscription = None;
        self.settings.about_open = false;
        self.settings.opened_from = self.keyboard.context;
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
            self.sync_settings_user_keys(cx);
        }
        self.settings.opened = true;
        // Every opening is a new showing, since a close runs its last sync while Settings still shows.
        self.settings.plugins.shown = false;
        let view = self.settings.panel.clone().expect("settings built");
        view.update(cx, |view, cx| {
            view.panel.facts.pane_path = pane_path;
            view.panel.side = settings_pages::Side::Pane;
            view.panel.show_section(view.panel.section);
            window.focus(&view.focus, cx);
            cx.notify();
        });
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
                self.settings.sheet_sources_subscription = None;
                let from = std::mem::take(&mut self.settings.opened_from);
                let mut focus = self.overlay_return_focus(cx);
                // v0.3.12 closes Settings and the key sheet opened from the shown rail back to its row.
                if focus == self.focus
                    && from == flea::keys::Context::Rail
                    && self.rail.as_ref().is_some_and(|join| {
                        join.host.read(cx).rail.mode() != crate::rail::Mode::Hidden
                    })
                {
                    focus = self.dual_focus.rail.clone();
                    // Synchronous, so a key sheet Settings hands over to keeps the keyboard.
                    self.focus_rail(true, cx);
                    self.keyboard.context = flea::keys::Context::Rail;
                }
                window.focus(&focus, cx);
                self.focus_trash_after_sheet(window, cx);
                if let Some(decision) = decision {
                    if self.trash_sheet_decision(&decision, window, cx) {
                        cx.notify();
                    } else {
                        self.settings_sheet_decision(decision, window, cx);
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
    // A write answer through this revision settles its pending, retried and Favorites entries.
    pub(crate) fn answered(&mut self, through: u64) {
        self.pending.retain(|(id, _)| *id > through);
        self.retried.retain(|(id, _)| *id > through);
        self.favourite_writes.retain(|id| *id > through);
    }

    pub(crate) fn row_states(&self, listing: usize, cursor: usize) -> crate::list::RowStates {
        let mut states = crate::list::RowStates::cursor_only(listing, cursor);
        states.focused = !self.opened && self.sheet.is_none();
        states
    }
}

#[cfg(test)]
mod density_prepare_tests;
#[cfg(test)]
mod favourites_reader_tests;
