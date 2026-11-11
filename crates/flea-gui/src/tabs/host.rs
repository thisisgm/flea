//! Runtime join for the tab state, one painted strip and the window's existing pane transport.
use super::{
    bar::{Bar, Labels, Panes},
    live::{Notice, Pane},
    state::{Live, Remember, TabState},
    transfer,
};
use crate::{
    state::Shell,
    window::{
        pane::PaneHost,
        startup::{Settings, View},
    },
};
use gpui::{Bounds, Context, Entity, Pixels, Task, Window};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub(crate) struct Host {
    pub tabs: [TabState; 2],
    pub bar: Bar,
    pub labels: Rc<RefCell<Labels>>,
    pub bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub capacity: Rc<Cell<bool>>,
    pub drop_zone: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub home: String,
    pub remember: Remember,
    pub menu: Option<super::context::Target>,
    pub notice: [Notice; 2],
    /// Tabs.js dropOverlay closes a side's Trash; the shell closes it once the tab action lands.
    pub trash_dropped: [bool; 2],
    pub view: Option<Entity<super::view::Strip>>,
    pub launch: transfer::Launch,
    pub timer: Option<Task<()>>,
    pub native: Option<Task<()>>,
    pub router: Option<crate::g2::drag_drop::router::Registration>,
    pub drag_serial: u64,
    pub landing_request: [Option<usize>; 2],
    pub snapshot: Option<super::atomic::Snapshot>,
    pub catchers: super::catcher::Catchers,
    pub geometry: transfer::Geometry,
    pub frames: Rc<RefCell<super::held::StripFrames>>,
    pub clients: super::geometry::Clients,
    /// Lifts that asked for the window rectangle, and the compositor client lists they asked, for tabTearoffState.
    pub lifts: usize,
    pub client_queries: usize,
    pub launch_started: bool,
    pub adopted_view: Option<&'static str>,
}

impl Host {
    pub fn new(settings: &Settings, path: &str) -> Self {
        Self {
            tabs: Default::default(),
            bar: Bar::new(std::process::id().to_string()),
            labels: Rc::new(RefCell::new(Labels::new(
                crate::list::Typeface::omarchy_default(),
            ))),
            capacity: Rc::new(Cell::new(true)),
            drop_zone: Rc::default(),
            bounds: Rc::default(),
            home: std::env::var("HOME").unwrap_or_default(),
            remember: Remember::seeded(&settings.data),
            menu: None,
            notice: Default::default(),
            trash_dropped: Default::default(),
            view: None,
            launch: transfer::Launch::read(|key| std::env::var(key).ok(), path),
            timer: None,
            native: None,
            router: None,
            drag_serial: 0,
            landing_request: Default::default(),
            snapshot: None,
            catchers: Default::default(),
            geometry: Default::default(),
            frames: Default::default(),
            clients: super::geometry::hyprland(),
            lifts: 0,
            client_queries: 0,
            launch_started: false,
            adopted_view: None,
        }
    }
}

struct World<'a> {
    tabs: &'a mut [TabState; 2],
    panes: &'a mut [Option<PaneHost>; 2],
    view: &'a mut View,
    settings: &'a Settings,
    home: &'a str,
    catalog: &'a flea::i18n::I18n,
    notice: &'a mut [Notice; 2],
    trash_dropped: &'a mut [bool; 2],
    adopted_view: &'a mut Option<&'static str>,
}

impl Panes for World<'_> {
    fn visit(&mut self, side: usize, f: &mut dyn FnMut(&mut TabState, &mut dyn Live)) -> bool {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return false;
        };
        f(
            &mut self.tabs[side],
            &mut Pane {
                host,
                view: self.view,
                settings: self.settings,
                home: self.home,
                catalog: self.catalog,
                notice: &mut self.notice[side],
                trash_dropped: &mut self.trash_dropped[side],
                adopted_view: self.adopted_view,
            },
        );
        true
    }
}

impl Shell {
    pub(crate) fn stop_tabs(&mut self) {
        self.strip.timer = None;
        self.strip.native = None;
        self.strip.router = None;
        self.strip.frames.borrow_mut().close();
        self.strip.capacity.set(false);
        self.strip.drop_zone.set(None);
        self.strip.geometry.cancel();
        self.strip.bar = Bar::new(std::process::id().to_string());
    }

    pub(crate) fn with_tabs<R>(&mut self, f: impl FnOnce(&mut Bar, &mut dyn Panes) -> R) -> R {
        let strip = &mut self.strip;
        // Only the action this call runs may close a Trash; a flag no tabs_changed consumed is stale.
        strip.trash_dropped = Default::default();
        let mut world = World {
            tabs: &mut strip.tabs,
            panes: &mut self.panes,
            view: &mut self.view,
            settings: &self.launch.settings,
            home: &strip.home,
            catalog: &self.locale.catalog,
            notice: &mut strip.notice,
            trash_dropped: &mut strip.trash_dropped,
            adopted_view: &mut strip.adopted_view,
        };
        strip.bar.set_pane(&mut world, Some(self.focus_side));
        let value = f(&mut strip.bar, &mut world);
        self.bind_tab_landings();
        value
    }

    /// Row and rail owners use this after validating their directory target; it uses the same tab model as t.
    pub fn open_tab(
        &mut self,
        side: usize,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if side >= 2 {
            return;
        }
        self.with_tabs(|_, panes| {
            panes.visit(side, &mut |tabs, pane| {
                tabs.open_new(pane, Some(path.clone()))
            });
        });
        self.tabs_changed(window, cx);
    }

    pub(crate) fn tab_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let side = self.focus_side;
        let taken = self.with_tabs(|_, panes| {
            let mut taken = false;
            panes.visit(side, &mut |tabs, pane| taken = tabs.act(action, pane));
            taken
        });
        if taken {
            self.tabs_changed(window, cx);
        }
        taken
    }

    pub(crate) fn tabs_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_tab_overlays(window, cx);
        for side in 0..2 {
            if std::mem::take(&mut self.strip.trash_dropped[side]) {
                self.drop_trash(side, window, cx);
            }
        }
        self.resolve_held_tab();
        if !self.strip.bar.out_active {
            self.strip.catchers.close(cx);
        }
        if self.strip.bar.drag_from.is_none() && !self.strip.bar.out_active {
            self.strip.geometry.cancel();
        }
        for side in 0..2 {
            self.run_navigation(side, window, cx);
        }
        self.remember_tabs();
        self.adopt_tab_view(window, cx);
        self.tab_timer(window, cx);
        self.publish_tab_notices(cx);
        cx.notify();
    }

    // Tabs.js sayCount: a verdict is a footer notice, and a count change withdraws the one standing.
    pub(crate) fn publish_tab_notices(&mut self, cx: &mut Context<Self>) {
        for side in 0..2 {
            let notice = &mut self.strip.notice[side];
            let withdraw = std::mem::take(&mut notice.withdraw)
                .then(|| notice.shown.take())
                .flatten();
            let owed = notice.owed.take();
            if withdraw.is_none() && owed.is_none() {
                continue;
            }
            let shown = self.status_messages(cx, |messages, now| {
                if let Some(serial) = withdraw {
                    messages.withdraw(serial, now);
                }
                owed.map(|text| {
                    messages.say(
                        &text,
                        false,
                        "",
                        flea::ui_logic_status_feedback::status::Hint::None,
                        now,
                    );
                    messages.notice_serial()
                })
            });
            if shown.is_some() {
                self.strip.notice[side].shown = shown;
            }
        }
    }

    pub(crate) fn remember_tabs(&mut self) {
        let side = self.focus_side;
        let mut saved = None;
        self.with_tabs(|_, panes| {
            panes.visit(side, &mut |tabs, pane| {
                saved = Some((pane.path(), tabs.remembered(pane), pane.busy()))
            });
        });
        let Some((path, plan, busy)) = saved else {
            return;
        };
        if let Some(patch) = self
            .strip
            .remember
            .owed(&path, &plan, self.view == View::Dual, busy)
        {
            if let Some(host) = &mut self.panes[side] {
                host.nav
                    .persist(patch, self.locale.catalog.tr("navigation.save_failed"));
            }
        }
    }

    pub(crate) fn tab_landed(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes[side]
            .as_ref()
            .is_none_or(|host| host.pane.busy())
        {
            return;
        }
        self.guard_tab_landing(side);
        self.with_tabs(|_, panes| {
            panes.visit(side, &mut |tabs, pane| tabs.apply_pending(pane));
        });
        let now = self.tab_now();
        self.with_tabs(|bar, panes| bar.drain_lifts(panes, now));
        self.settle_tear_off_launch(side);
        self.run_navigation(side, window, cx);
        self.remember_tabs();
        self.adopt_tab_view(window, cx);
        self.pump_tab_acks(cx);
        self.publish_tab_notices(cx);
    }

    pub(crate) fn settle_tear_off_launch(&mut self, side: usize) {
        let Some(host) = &self.panes[side] else {
            return;
        };
        if host.pane.busy() {
            return;
        }
        let success = matches!(
            host.pane.state.kind,
            crate::pane::Kind::Ready | crate::pane::Kind::Empty
        );
        let locating = self.strip.tabs[side]
            .tabs
            .as_ref()
            .is_some_and(|tabs| !tabs.pending.locate_path.is_empty());
        if locating {
            return;
        }
        if success {
            if let Some((pid, token)) = self.strip.launch.opened(&host.path) {
                self.strip.bar.send_taken(&pid, &token);
            }
        }
    }

    pub(crate) fn tab_located(&mut self, side: usize, located: &crate::backend::Located) -> bool {
        if located.id.is_some() {
            return false;
        }
        let pending = self.strip.tabs[side].tabs.as_ref().is_some_and(|tabs| {
            tabs.pending.locate_path == located.path && !located.path.is_empty()
        });
        self.with_tabs(|_, panes| {
            panes.visit(side, &mut |tabs, pane| {
                tabs.located(
                    pane,
                    &located.path,
                    &located.directory,
                    usize::try_from(located.index).ok(),
                )
            });
        });
        pending
    }

    /// A peeked line is every owner's to read; the strip takes only the reserved check it asked for.
    pub(crate) fn tab_peeked(
        &mut self,
        side: usize,
        document: &flea::jsondoc::Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(reply) = transfer::peeked(document) else {
            return;
        };
        if self.strip.bar.pending.is_none() {
            return;
        }
        self.with_tabs(|bar, panes| {
            bar.on_peeked(
                panes,
                side,
                &reply.path,
                reply.hidden,
                reply.read_failed,
                reply.hidden_last,
                reply.first,
            )
        });
        self.tabs_changed(window, cx);
        self.pump_tab_acks(cx);
    }

    fn pump_tab_acks(&mut self, cx: &mut Context<Self>) {
        let Some((pid, token)) = self.strip.bar.pump_taken() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .spawn(async move {
                    let reply = crate::ipc::tabs::send_taken(&pid, &token);
                    super::trace::stage("taken-sent", || {
                        let said = format!("{}{}", reply.stdout, reply.stderr);
                        format!(
                            "target={pid} token={token} code={} reply={}",
                            reply.code,
                            said.trim()
                        )
                    });
                })
                .await;
            let _ = this.update(cx, |shell, cx| {
                shell.strip.bar.ack_running = false;
                shell.pump_tab_acks(cx);
            });
        })
        .detach();
    }

    pub(crate) fn tab_timer(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.strip.timer.is_some()
            || (self.strip.bar.pending.is_none() && self.strip.bar.lifts.is_empty())
        {
            return;
        }
        self.strip.timer = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(super::bar::ACK_POLL_MS))
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |shell, cx| {
                    shell.strip.timer = None;
                    let now = shell.panes[shell.focus_side]
                        .as_ref()
                        .map_or(0, |host| host.pane.now());
                    shell.with_tabs(|bar, panes| {
                        bar.expire(panes, now);
                        bar.drain_lifts(panes, now);
                    });
                    shell.tabs_changed(window, cx);
                })
            });
        }));
    }
}
