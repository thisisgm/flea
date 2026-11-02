//! PickerPlaces' shares and phones reuse the app's GVFS opener and the device source's GIO listing.
use super::Picker;
use crate::g2::pl1::{process::Runner, secret::Secret};
use crate::g2::pl2::{
    browser::{Activated, Closed, ShareView},
    mount::{Effect, Request},
    service::{Service, ServiceEvent},
};
use flea::ui_logic_pl4::{
    order::RailSection,
    rail::{Group, RailEntry},
};
use gpui::{prelude::*, Context, Entity, Subscription, Task};
use std::{path::PathBuf, rc::Rc};

pub(super) struct Network {
    pub rows: Vec<RailSection>,
    pub bookmarks: PathBuf,
    mountinfo: PathBuf,
    pub shares: Entity<ShareView>,
    service: Entity<Service>,
    origin: u64,
    waiting: Option<u64>,
    read: Option<Task<()>>,
    tasks: Vec<Task<()>>,
    _events: Vec<Subscription>,
}

impl Picker {
    pub(super) fn start_network(&mut self, runner: Rc<dyn Runner>, cx: &mut Context<Self>) {
        let service = cx.new(|_| {
            Service::new(
                runner,
                std::env::var("XDG_RUNTIME_DIR").unwrap_or_default(),
                std::env::var("FLEA_GVFS_FUSE").unwrap_or_default(),
            )
        });
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette =
            crate::g2::op1::palette::CardPalette::from_theme(&Default::default(), theme, 0.);
        let shares = cx.new(|cx| ShareView::new(self.locale.catalog.clone(), palette, cx));
        let events = vec![
            cx.subscribe(&service, |picker, _, event: &ServiceEvent, cx| {
                picker.network_effect(&event.0, cx)
            }),
            cx.subscribe(&shares, |picker, _, event: &Activated, cx| {
                if let Some(network) = &picker.rail.network {
                    let service = network.service.clone();
                    let (uri, label, origin) = (event.0.clone(), event.1.clone(), event.2);
                    service.update(cx, |service, cx| {
                        service.open_child_share(uri, label, origin, cx)
                    });
                }
            }),
            cx.subscribe(&shares, |picker, _, event: &Closed, cx| {
                if let Some(network) = &picker.rail.network {
                    network
                        .service
                        .clone()
                        .update(cx, |service, _| service.close_browser(event.0));
                }
                if let Some(window) = picker.browse.window {
                    let focus = picker.rail.focus.clone();
                    let _ = window.update(cx, |_, window, cx| window.focus(&focus, cx));
                }
            }),
        ];
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&self.browse.home).join(".config"));
        self.rail.network = Some(Network {
            service,
            shares,
            rows: Vec::new(),
            origin: 0,
            waiting: None,
            read: None,
            tasks: Vec::new(),
            bookmarks: if cfg!(test) {
                PathBuf::new()
            } else {
                config.join("gtk-3.0/bookmarks")
            },
            mountinfo: if cfg!(test) {
                PathBuf::new()
            } else {
                "/proc/self/mountinfo".into()
            },
            _events: events,
        });
    }

    // Every completed GIO poll also re-reads saved places; no stat of a remote mountpoint is needed.
    pub(super) fn read_network(&mut self, cx: &mut Context<Self>) {
        let Some(network) = &mut self.rail.network else {
            return;
        };
        let gio = self
            .rail
            .devices
            .as_ref()
            .map(|source| source.read(cx).mount_listing().to_owned())
            .unwrap_or_default();
        let bookmarks = network.bookmarks.clone();
        let mountinfo = network.mountinfo.clone();
        let read = cx.background_spawn(async move {
            let marks = std::fs::read_to_string(bookmarks).unwrap_or_default();
            let info = std::fs::read_to_string(mountinfo).unwrap_or_default();
            share_rows(&gio, &marks, &info)
        });
        network.read = Some(cx.spawn(async move |this, cx| {
            let rows = read.await;
            let _ = this.update(cx, |picker, cx| {
                if let Some(network) = &mut picker.rail.network {
                    network.rows = rows;
                    picker.rebuild_rail(cx);
                }
            });
        }));
    }

    pub(super) fn open_network(&mut self, entry: &RailEntry, cx: &mut Context<Self>) {
        let Some(network) = &mut self.rail.network else {
            return;
        };
        network.origin += 1;
        network.waiting = Some(network.origin);
        let request = Request {
            id: 0,
            uri: if entry.group == Group::Favourite {
                entry.path.clone()
            } else {
                entry.uri.clone()
            },
            label: entry.label.clone(),
            origin: network.origin,
            mounted: entry.mounted == Some(true),
            authenticated: false,
            password: Secret::new(),
        };
        network.service.clone().update(cx, |service, cx| {
            let now = service.now(cx);
            service.open_share(request, now, cx);
        });
    }

    pub(super) fn cancel_network(&mut self, cx: &mut Context<Self>) {
        if let Some(network) = &mut self.rail.network {
            network.shares.update(cx, |shares, cx| shares.close(cx));
            if let Some(origin) = network.waiting.take() {
                network
                    .service
                    .update(cx, |service, cx| service.cancel_origin(origin, cx));
            }
        }
    }

    fn network_waits(&self, origin: u64) -> bool {
        self.rail
            .network
            .as_ref()
            .is_some_and(|network| network.waiting == Some(origin))
            && !self.backend_unavailable()
            && !self.answered()
    }

    fn network_effect(&mut self, effect: &Effect, cx: &mut Context<Self>) {
        match effect {
            Effect::Open { path, origin, .. } | Effect::OpenFile { path, origin, .. }
                if self.network_waits(*origin) =>
            {
                if let Some(network) = self.rail.network.as_mut() {
                    network.waiting = None;
                }
                if matches!(effect, Effect::OpenFile { .. }) {
                    self.path_out(
                        crate::path::view::Out::Open {
                            path: path.clone(),
                            side: 0,
                        },
                        cx,
                    );
                } else {
                    self.rail_open(path, cx);
                }
            }
            Effect::RefreshMounts => self.refresh_mounts(cx),
            Effect::Say { note, origin } if self.network_waits(*origin) => {
                let text = crate::g2::pl1::words::say(&self.locale.catalog, note);
                self.say(
                    text.split_whitespace().collect::<Vec<_>>().join(" "),
                    true,
                    cx,
                );
            }
            Effect::Retry {
                origin,
                error,
                args,
                ..
            } if self.network_waits(*origin) => {
                if let Some(network) = self.rail.network.as_mut() {
                    network.waiting = None;
                }
                let text = self
                    .locale
                    .catalog
                    .tr_args(error, &args.iter().map(String::as_str).collect::<Vec<_>>());
                self.say(
                    text.split_whitespace().collect::<Vec<_>>().join(" "),
                    true,
                    cx,
                );
            }
            Effect::Peek {
                generation,
                path,
                count,
            } => self.network_peek(*generation, path, *count, cx),
            Effect::Shares {
                uri,
                label,
                names,
                origin,
            } if self.network_waits(*origin) => {
                if let (Some(window), Some(network)) =
                    (self.browse.window, self.rail.network.as_ref())
                {
                    let shares = network.shares.clone();
                    let (uri, label, names, origin) =
                        (uri.clone(), label.clone(), names.clone(), *origin);
                    let _ = window.update(cx, |_, window, cx| {
                        shares.update(cx, |shares, cx| {
                            shares.show(uri, label, names, origin, window, cx)
                        })
                    });
                    cx.notify();
                }
            }
            _ => {}
        }
    }

    fn network_peek(&mut self, generation: u64, path: &str, count: usize, cx: &mut Context<Self>) {
        let Some(network) = &mut self.rail.network else {
            return;
        };
        let service = network.service.clone();
        let path = path.to_owned();
        let read = cx.background_spawn({
            let path = path.clone();
            async move {
                match std::fs::read_dir(&path) {
                    Ok(entries) => (
                        entries
                            .filter_map(Result::ok)
                            .take(count)
                            .map(|entry| entry.file_name().to_string_lossy().into_owned())
                            .collect::<Vec<_>>(),
                        false,
                    ),
                    Err(_) => (Vec::new(), true),
                }
            }
        });
        network.tasks.retain(|task| !task.is_ready());
        network.tasks.push(cx.spawn(async move |_, cx| {
            let (names, failed) = read.await;
            let login = std::env::var("USER").unwrap_or_default();
            service.update(cx, |service, cx| {
                let now = service.now(cx);
                service.peeked(generation, &path, &names, failed, &login, now, cx);
            });
        }));
    }

    pub(super) fn stop_places(&mut self, cx: &mut Context<Self>) {
        self.cancel_network(cx);
        self.rail.recent_read = None;
        if let Some(source) = self.rail.devices.clone() {
            source.update(cx, |source, cx| source.set_polling(false, cx));
        }
        if let Some(network) = &mut self.rail.network {
            network.read = None;
            let service = network.service.clone();
            network
                .tasks
                .push(service.update(cx, |service, cx| service.shutdown(cx)));
        }
    }

    pub(super) fn network_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(shares) = self
            .rail
            .network
            .as_ref()
            .map(|network| network.shares.clone())
            .filter(|shares| shares.read(cx).model.open)
        else {
            return false;
        };
        let action = match event.keystroke.key.as_str() {
            "escape" | "left" | "backspace" => "escape",
            "down" | "j" => "cursorDown",
            "up" | "k" => "cursorUp",
            "enter" | "space" | "right" | "l" => "open",
            _ => return true,
        };
        shares.update(cx, |shares, cx| {
            shares.action(action, cx);
        });
        true
    }
}

// PickerPlaces lists shares, never the gvfs FUSE root every share sits under (EY-506).
pub(super) fn share_rows(gio: &str, marks: &str, mountinfo: &str) -> Vec<RailSection> {
    let mut rows = flea::ui_logic_pl2::rail::rebuild(
        gio,
        &flea::ui_logic_pl2::mounts::non_file_bookmarks(marks),
        mountinfo,
    );
    rows.retain(|row| row.fstype != "fuse.gvfsd-fuse");
    crate::window::network::section(&rows, true)
}
