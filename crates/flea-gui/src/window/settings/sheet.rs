//! The keyboard sheet reads the live rail and history, then hands each result to its original window owner.
use crate::{
    g2::st1::{Request, Sheet, SheetView, Sources},
    state::Shell,
};
use flea::ui_logic_pl4::rail::{Group, Kind};
use flea::ui_logic_st1::sheet::{self, Decision, RailEntry};
use gpui::{App, AppContext, Context, Window};

impl Shell {
    fn sheet_sources(&self, cx: &App) -> Sources {
        let mut sources = Sources {
            home: std::env::var("HOME").unwrap_or_default(),
            menus: self.sheet_menus(),
            ..Sources::default()
        };
        if let Some(join) = &self.rail {
            let producer = join.places.read(cx);
            sources.rail = producer
                .entries()
                .iter()
                .map(|entry| RailEntry {
                    label: crate::g2::pl4::content::row_label(&self.locale.catalog, entry)
                        .to_string(),
                    group: match entry.group {
                        Group::Favourite | Group::Favorite => "favourite",
                        Group::Network => "network",
                        Group::Device => "device",
                        _ => "home",
                    }
                    .into(),
                    kind: if entry.kind == Kind::Recent {
                        "recent"
                    } else {
                        ""
                    }
                    .into(),
                    identity: format!("{:?}", entry.identity()),
                })
                .collect();
            sources.recent = producer
                .places
                .read(cx)
                .history
                .iter()
                .map(|visit| visit.path.clone())
                .collect();
        }
        sources
    }

    fn refresh_sheet_sources(&mut self, cx: &mut Context<Self>) {
        let sources = self.sheet_sources(cx);
        if let Some(view) = &self.settings.sheet {
            view.update(cx, |view, cx| {
                view.sheet.set_sources(sources);
                cx.notify();
            });
        }
    }

    pub(crate) fn open_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.opened = false;
        self.settings.opened_from = self.keyboard.context;
        self.keyboard.disarm();
        let preset =
            flea::ui_logic_st1::pages::word(&self.launch.settings.data, &["keys"], "default");
        let mut sheet = Sheet::new(preset, self.locale.catalog.clone(), self.sheet_sources(cx));
        self.prepare_trash_sheet(&mut sheet, cx);
        sheet.keyboard = self.sheet_keyboard(preset, window.layout_mirrored(), cx);
        let corner = self.corner;
        let typeface = self.settings_typeface(cx);
        let view = cx.new(|cx| {
            let mut view = SheetView::new(sheet, cx);
            view.corner = corner;
            view.typeface = typeface;
            window.focus(&view.focus, cx);
            view.forget_on_blur(window, cx);
            view
        });
        self.settings.sheet_subscription =
            Some(
                cx.subscribe_in(&view, window, |shell, _, request: &Request, window, cx| {
                    shell.settings_request(request.clone(), window, cx);
                }),
            );
        self.settings.sheet = Some(view);
        self.settings.sheet_sources_subscription = self.rail.as_ref().map(|join| {
            let producer = join.places.clone();
            let subscription = cx.observe_in(&producer, window, |shell, _, _, cx| {
                shell.refresh_sheet_sources(cx)
            });
            producer.update(cx, |producer, cx| {
                producer.load_places(cx);
                let path = flea::ui_logic_pl4::recent::history_path(
                    &std::env::var("XDG_DATA_HOME").unwrap_or_default(),
                    &std::env::var("HOME").unwrap_or_default(),
                );
                producer
                    .places
                    .update(cx, |places, cx| places.reread_history(path, cx));
            });
            subscription
        });
        cx.notify();
    }

    pub(super) fn settings_sheet_decision(
        &mut self,
        decision: Decision,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match decision {
            Decision::Action { action } => self.settings_host_action(&action, window, cx),
            Decision::Menu { menu_action } | Decision::Confirm { menu_action } => {
                self.menu_keyboard_open(window, cx);
                if self.menu.captured.is_some() {
                    self.menu.from_sheet = true;
                    self.menu.presentation.cancel();
                    self.menu.menu.borrow_mut().close();
                    let (action, leaf) = menu_action.split_once(':').unwrap_or((&menu_action, ""));
                    self.menu.pending = Some((action.into(), leaf.into()));
                    self.activate_menu(window, cx);
                }
            }
            decision @ Decision::Place { .. } => {
                let sources = self.sheet_sources(cx);
                if let Some(index) = sheet::place_index(&sources.rail, &decision) {
                    if let Some(join) = &self.rail {
                        join.places.update(cx, |producer, cx| {
                            producer.side = self.focus_side;
                            producer.activate(index, cx);
                        });
                    }
                    self.route_rail(window, cx);
                }
            }
            Decision::Recent { path } => self.open_file(self.focus_side, path, window, cx),
            Decision::Disabled | Decision::Nothing => {}
        }
    }
}
