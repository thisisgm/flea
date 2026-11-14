//! The live Columns join: committed pane snapshots, watched peeks and one retained painter per drawn track.
use super::{pane::PaneHost, startup::View};
use crate::{
    columns::{paint::Painters, peeks::Lens, preview_host::ThirdSwap, Area, PaneView, Ports},
    g2::status_feedback::{hero::EmptyHero, state::StateMessage},
    navigation::Navigator,
    pane::Pane,
    state::Shell,
};
use gpui::{Context, Entity, Task, Window};

#[path = "columns_edge.rs"]
mod edge;
#[path = "columns_input.rs"]
mod input;
#[path = "columns_keys.rs"]
mod keys;
#[path = "columns_menu.rs"]
mod menu;
pub use input::BackgroundMenu;
#[path = "columns_gestures.rs"]
mod gestures;
pub(crate) use gestures::{drop_spot, hover_at, on_name, spring_spot};
#[path = "columns_events.rs"]
mod events;
#[cfg(test)]
#[path = "columns_scroll_lane_tests.rs"]
mod lane_tests;
#[path = "columns_loading.rs"]
mod loading;
#[path = "columns_readers.rs"]
pub(crate) mod readers;
#[path = "columns_render.rs"]
mod render;
#[path = "columns_requested.rs"]
mod requested;
#[path = "columns_scroll.rs"]
mod scroll;
#[path = "columns_scroll_bar.rs"]
mod scroll_bar;
#[path = "columns_scroll_input.rs"]
mod scroll_input;
#[cfg(test)]
#[path = "columns_scroll_state_tests.rs"]
mod scroll_tests;
#[path = "columns_shown.rs"]
mod shown;
pub(crate) use requested::requested;

#[cfg(test)]
#[path = "columns_review_r2_tests.rs"]
mod review_r2;

pub(crate) struct Columns {
    /// The neighbour file a first tap selected, which the same gesture's second tap opens wherever the tracks moved.
    pub file_tap: Option<String>,
    pub area: Area,
    pub preview_hosted: bool,
    pub painters: Painters,
    pub(crate) scroll: [scroll::Scrolling; 5],
    pub heroes: [Option<Entity<EmptyHero>>; 5],
    pub notices: [Option<Entity<StateMessage>>; 5],
    pub loading: [Option<std::rc::Rc<crate::feedback::spinner::Crawl>>; 5],
    loading_since: [std::rc::Rc<std::cell::Cell<Option<u64>>>; 5],
    paint_pending: std::cell::Cell<bool>,
    pub shown: [Option<(String, crate::columns::TrackState, u64)>; 5],
    pub timer: Option<(u64, Task<()>)>,
    /// The switch into Columns is held on the List until every drawn column answers or CAP_MS passes.
    pub entering: bool,
    arm_entry: bool,
    pub counters: Counters,
    /// The track row under the pointer, drawn as the hover plate of its row.
    pub hover: Option<gestures::Hover>,
    /// The track row the left button last went down on and the keys held then, so a drag lifts the pressed row and not the one its motion reached.
    pub(crate) pressed: Option<(gestures::Hover, gpui::Modifiers)>,
    path: String,
    seen: Option<Seen>,
    held: std::rc::Rc<crate::list::pane::Held>,
}

/// The Columns surface's swap counters, as the pane's own: a hold per entry kept on the List, a fallback per entry that
/// outlasted its cap, a blank frame per frame that drew an unanswered track with no tile.
#[derive(Debug, Default)]
pub struct Counters {
    pub holds: usize,
    pub fallbacks: usize,
    pub blank_frames: usize,
}

#[derive(Clone, Copy, PartialEq)]
struct Seen {
    cursor: usize,
    listing: usize,
    first: usize,
    in_flight: bool,
    hidden: bool,
    hidden_last: bool,
    visible: bool,
    width: f32,
    height: f32,
    revision: u64,
    orders: [Option<crate::navigation::folder_state::Order>; 5],
    // The filter's drawn rows by identity, so a new answer under an unmoved cursor re-lays the columns.
    filter: Option<*const usize>,
}

impl Columns {
    fn new(width: f32, height: f32, settings: &super::startup::Settings) -> Self {
        let share = f64::from(super::selection_preview::host::share(&settings.data));
        let mut area = Area::new(
            width,
            height,
            Some(settings.columns as f64),
            share,
            super::selection_preview::host::settings(&settings.data),
        );
        area.set_weights(super::selection_preview::host::tracks(&settings.data));
        Self {
            file_tap: None,
            area,
            preview_hosted: false,
            painters: Painters::default(),
            scroll: Default::default(),
            heroes: Default::default(),
            notices: Default::default(),
            loading: Default::default(),
            loading_since: Default::default(),
            paint_pending: Default::default(),
            shown: Default::default(),
            timer: None,
            entering: false,
            arm_entry: true,
            counters: Counters::default(),
            hover: None,
            pressed: None,
            path: String::new(),
            seen: None,
            held: Default::default(),
        }
    }
}

impl Columns {
    /// Settings' Columns view limit: re-laid now, and the next sync asks the columns the new count shows.
    pub(crate) fn set_limit(&mut self, limit: Option<f64>) {
        if self.area.limit != limit {
            self.area.set_limit(limit);
            self.seen = None;
        }
    }

    /// The entry hold ends with the last answer or the landing's cap, which leaves a still-out column as its loading tile.
    fn settle_entry(&mut self, pane: &PaneView<'_>, ports: &mut Ports<'_>, now: u64) {
        if !self.entering {
            return;
        }
        let out = self.unanswered(&pane.lens);
        self.area.landing.retarget_entry(pane.lens.here, &out);
        self.area.landing.tick(now);
        let out = !out.is_empty();
        if out && self.area.landing.cap_deadline().is_some() {
            return;
        }
        if out && self.area.landing.loading(&pane.child(), pane.lens.here) {
            self.area.host.fallback_at = Some(now);
            self.area.tick(pane, ports, now);
        }
        self.entering = false;
        self.counters.fallbacks += usize::from(out);
    }

    /// The drawn column directories whose reply has not landed under the folder's order.
    fn unanswered(&self, lens: &Lens<'_>) -> Vec<String> {
        let peeks = &self.area.peeks;
        let out = |path: &&String| !peeks.answered(&lens.key(path), path);
        self.area.drawn.iter().filter(out).cloned().collect()
    }

    /// A newly visible area asked for everything: columns already answered need no hold.
    fn hold_entry(&mut self, pane: &PaneView<'_>, now: u64) {
        let out = self.unanswered(&pane.lens);
        self.area.landing.enter(pane.lens.here, &out, now);
        self.entering = !out.is_empty();
        self.counters.holds += usize::from(self.entering);
    }
}

fn with_pane<R>(
    pane: &Pane,
    nav: &Navigator,
    shown: Option<&[usize]>,
    read: impl FnOnce(PaneView<'_>) -> R,
) -> R {
    let folders = nav.folders.borrow();
    // ColumnsArea.qml cursorRow: a cursor the filter hides has no row, so no third column follows it.
    let hidden = shown.is_some_and(|rows| !rows.contains(&pane.cursor));
    let orders = |path: &str| folders.peek_order(path);
    read(PaneView {
        lens: Lens {
            here: pane.path(),
            hidden: folders.hidden,
            hidden_last: folders.hidden_last,
            first: pane.first_count(),
            orders: &orders,
        },
        base: pane.path(),
        cursor_row: pane.held.row(pane.cursor).filter(|_| !hidden),
        cursor: pane.cursor,
        listing: pane.held.listing,
        in_flight: pane.busy(),
        filter_hid_cursor: hidden && shown.is_some_and(|rows| !rows.is_empty()),
    })
}

fn with_area<R>(
    host: &mut PaneHost,
    change: impl FnOnce(&mut Columns, PaneView<'_>, &mut Ports<'_>) -> R,
) -> Option<R> {
    let now = host.pane.now();
    let columns = host.columns.as_mut()?;
    let mut swap = host.preview_port.clone();
    let mut preview = host.preview_port.clone();
    let shown = host.filter.shown.clone();
    Some(with_pane(&host.pane, &host.nav, shown.as_deref(), |pane| {
        let revision = columns.area.landing.version;
        let tracks = std::rc::Rc::clone(columns.area.tracks());
        for track in tracks
            .iter()
            .filter(|track| track.state == crate::columns::TrackState::Loading)
        {
            if let Some(at) = columns.loading_since[track.slot.index()].get() {
                columns
                    .area
                    .landing
                    .mark_shown(&track.path, true, pane.lens.here, at);
            }
        }
        columns.area.landing.tick(now);
        let file = pane.cursor_row.is_some_and(|row| !row.dir);
        let held = &columns.area.host.shown;
        // A hold that ends unseen still shows the row as it stands: a file, or none once a filter matches nothing.
        let none = shown.as_deref().is_some_and(<[usize]>::is_empty) && held.has_row;
        if swap.idle() && ((file && (held.is_dir || !held.has_row)) || (!file && none)) {
            columns.area.host.show_cursor_row(
                crate::columns::preview_host::Cursor {
                    row: file,
                    is_dir: false,
                    child: "",
                },
                true,
                false,
            );
            columns.seen = None;
        }
        let mut ports = Ports {
            swap: &mut swap,
            preview: &mut preview,
        };
        let done = change(columns, pane, &mut ports);
        columns.preview_hosted = ports.preview.ipc("previewColumnState", &[]).is_some();
        columns.settle_entry(&pane, &mut ports, now);
        // A draw can win the deadline race: publish its landing transition before the timer is discarded.
        if columns.area.landing.version != revision {
            columns.area.sync(&pane, &mut ports, now);
            columns.seen = None;
        }
        done
    }))
}

fn flush(host: &mut PaneHost, locale: &super::locale::Snapshot) {
    let Some(columns) = host.columns.as_mut() else {
        return;
    };
    for line in std::mem::take(&mut columns.area.outbox) {
        if let Some(backend) = host.backend.as_ref().filter(|_| !host.stopped) {
            if let Err(error) = backend.columns_line(line) {
                host.stopped = true;
                host.record_error(error, locale);
                break;
            }
        }
    }
}

impl Shell {
    pub(crate) fn sync_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let geometry = self.geometry(window);
        let visible = self.view == View::Columns;
        let preview_settings = super::selection_preview::host::settings(&self.launch.settings.data);
        let preview_share = f64::from(super::selection_preview::host::share(
            &self.launch.settings.data,
        ));
        let preview_tracks = super::selection_preview::host::tracks(&self.launch.settings.data);
        let limit = Some(self.launch.settings.columns as f64);
        let width = geometry.panes[0].width;
        let height =
            (geometry.panes[0].height - self.slots.strip_height - self.columns_search_strip(0))
                .max(0.);
        let Some(host) = self.panes[0].as_mut() else {
            return;
        };
        if visible {
            let landing = host.landing_height(
                View::Columns,
                geometry.content_height(View::List, self.slots.strip_height),
                height,
            );
            host.pane.set_height(landing);
        }
        if host.columns.is_none() && visible {
            host.columns = Some(Columns::new(width, height, &self.launch.settings));
        }
        if let Some(columns) = &mut host.columns {
            if columns.area.share != preview_share {
                columns.area.set_share(preview_share);
                columns.seen = None;
            }
            if columns.area.weights != preview_tracks {
                columns.area.set_weights(preview_tracks);
                columns.seen = None;
            }
            // ColumnsArea.columnsLimit: a Settings change re-lays the drawn columns live.
            columns.set_limit(limit);
            if columns.area.settings != preview_settings {
                columns.area.settings = preview_settings;
                columns.seen = None;
            }
        }
        let revision = self.locale.revision;
        let now = host.pane.now();
        let held = host.pane.held.clone();
        let filter = host.filter.shown.as_ref().map(|rows| rows.as_ptr());
        with_area(host, |columns, pane, ports| {
            let key = Seen {
                cursor: pane.cursor,
                listing: pane.listing,
                first: pane.lens.first,
                in_flight: pane.in_flight,
                hidden: pane.lens.hidden,
                hidden_last: pane.lens.hidden_last,
                visible,
                width,
                height,
                revision,
                orders: std::array::from_fn(|at| {
                    columns
                        .area
                        .drawn
                        .get(at)
                        .and_then(|path| (pane.lens.orders)(path))
                }),
                filter,
            };
            if columns.seen == Some(key)
                && columns.path == pane.lens.here
                && std::rc::Rc::ptr_eq(&columns.held, &held)
            {
                return;
            }
            columns.seen = Some(key);
            columns.held = held;
            let was_visible = columns.area.visible;
            if !visible {
                columns.scroll = Default::default();
                columns.area.set_visible(false, &pane, ports, now);
                columns.area.drawn.clear();
                columns.timer = None;
                columns.entering = false;
                columns.arm_entry = true;
                columns.painters.park();
                for since in &columns.loading_since {
                    since.set(None);
                }
                columns.paint_pending.set(false);
                for hero in columns.heroes.iter().flatten() {
                    hero.update(cx, |hero, cx| hero.hide(cx));
                }
                columns.shown = Default::default();
                return;
            }
            columns.area.resize(width, height);
            if !was_visible {
                columns.area.set_visible(true, &pane, ports, now);
            }
            if columns.path != pane.lens.here {
                if !columns.path.is_empty() {
                    columns.area.path_landed(&pane, ports, now);
                }
                columns.path = pane.lens.here.to_owned();
            }
            columns.area.orders_changed(&pane.lens);
            columns.area.flight_ended(&pane, ports, now);
            columns.area.sync(&pane, ports, now);
            if std::mem::take(&mut columns.arm_entry) {
                columns.hold_entry(&pane, now);
            }
        });
        flush(host, &self.locale);
        self.arm_columns(window, cx);
        self.persist_columns_edge(0, window, cx);
    }
}
