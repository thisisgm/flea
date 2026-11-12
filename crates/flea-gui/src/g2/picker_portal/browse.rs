//! The chooser's folder on its listing child: generation-matched replies, the typed-path route and row activation.
use super::filename::Filename;
use super::picker::Picker;
use crate::g2::picker_listing::chrome::PathBar;
use crate::g2::picker_listing::view::{ListingView, SaveView};
use crate::g2::picker_listing::{Escape, SaveField};
use crate::list::Typeface;
use crate::path::view::{Inputs, Out, PathView};
use flea::ui_logic_picker_listing::FilterDef;
use flea::ui_logic_picker_portal as logic;
use gpui::{prelude::*, AnyWindowHandle, Context, Entity, Subscription};

// The rows a listing asks for with its folder, and the window a cursor outside them asks for.
pub const FIRST: usize = 128;

// A typed path waiting for its peek, then a typed file waiting for its row.
#[derive(Default)]
pub(super) struct Route {
    pub(super) asked: String,
    pub(super) leaf: String,
    pub(super) leaf_parent: String,
    pub(super) leaf_index: i64,
}

pub struct Browse {
    pub listing: Entity<ListingView>,
    pub path: Entity<PathBar>,
    pub filename: Option<Entity<Filename>>,
    pub(super) home: String,
    // List requests the child has not answered yet; only the answer to the newest names the current generation.
    pub(super) in_flight: usize,
    pub(super) reply_gen: u64,
    pub(super) replied: bool,
    pub(super) route: Route,
    pub(super) window: Option<AnyWindowHandle>,
    pub(super) focus_filename: bool,
    pub(super) stems_seen: u64,
    _path: Subscription,
    _pictures: Subscription,
    _filename: Option<Subscription>,
}

impl Browse {
    pub fn new(
        req: &logic::Request,
        home: &str,
        catalog: &flea::i18n::I18n,
        cx: &mut Context<Picker>,
    ) -> Self {
        let typeface = cx
            .try_global::<Typeface>()
            .cloned()
            .unwrap_or_else(Typeface::omarchy_default);
        let listing = cx.new(|_| ListingView::new(catalog.clone(), typeface));
        listing.update(cx, |view, _| {
            view.set_marks(Default::default(), req.multiple && !logic::saving(req));
            view.configure_picker(logic::folder_mode(req));
        });
        let pictures = cx.subscribe(
            &listing,
            |picker: &mut Picker, _, line: &crate::g2::picker_listing::pictures::RequestLine, _| {
                picker.send_listing(&line.0);
            },
        );
        let path = cx.new(|cx| PathBar::new(catalog, cx));
        let view = path.read(cx).path();
        let subscription = cx.subscribe(&view, |picker: &mut Picker, _, out: &Out, cx| {
            picker.path_out(out.clone(), cx)
        });
        let filename = logic::saving(req).then(|| {
            let seed = logic::initial_save_name(req).to_owned();
            let save = cx.new(|_| SaveView::new(catalog.clone(), SaveField::new(&seed)));
            let label = catalog.tr("picker.browser.save_as");
            let tracking = crate::g2::status_feedback::caption::tracking_for(catalog.language());
            let menu = std::sync::Arc::new(catalog.clone());
            cx.new(|cx| Filename::new(&seed, label, tracking, save, cx).with_catalog(menu))
        });
        let filename_subscription = filename.as_ref().map(|field| {
            cx.subscribe(field, |picker: &mut Picker, _, event, cx| {
                if let super::filename::FieldKey::Edited(text) = event {
                    picker.save_name_edited(text.clone(), cx);
                }
            })
        });
        Self {
            _filename: filename_subscription,
            listing,
            path,
            focus_filename: filename.is_some(),
            filename,
            home: home.to_owned(),
            in_flight: 0,
            reply_gen: 0,
            replied: false,
            route: Route::default(),
            window: None,
            stems_seen: 0,
            _path: subscription,
            _pictures: pictures,
        }
    }

    pub fn path_view(&self, cx: &gpui::App) -> Entity<PathView> {
        self.path.read(cx).path()
    }
}

impl Picker {
    // Sample input: {"c":"list","path":"/d","first":128,"hidden":false}
    // request_line and paths_line end on the object's closing brace, with no newline.
    pub(super) fn listing_request(&self, mut line: String, cx: &gpui::App) -> String {
        let generation = self.browse.listing.read(cx).generation();
        line.insert_str(line.len() - 1, &format!(",\"pickerGen\":{generation}"));
        line
    }

    pub(super) fn send_listing(&self, line: &str) -> bool {
        self.listing_child.send(line)
    }

    // The caller's chosen filter rides on every list request, so the backend never sends a row it excludes.
    fn chip_filter(&self) -> Option<FilterDef> {
        usize::try_from(self.listing.chip)
            .ok()
            .and_then(|at| self.req.filters.get(at))
            .map(|filter| FilterDef {
                label: filter.label.clone(),
                globs: filter.globs.clone(),
                mimes: filter.mimes.clone(),
            })
    }

    // Only an absolute folder lists, or Recent, whose paths the history file names.
    pub fn open_folder(&mut self, path: &str, cx: &mut Context<Self>) {
        self.open_folder_travel(path, super::navigation::Travel::Open, cx);
    }

    pub(super) fn open_folder_travel(
        &mut self,
        path: &str,
        travel: super::navigation::Travel,
        cx: &mut Context<Self>,
    ) {
        let recent = logic::is_recent(path);
        if !path.starts_with('/') && !recent {
            return;
        }
        // A folder change ends any drive wait, as v0.3.12 PickerPlaces.qml onPathChanged does.
        if path != self.listing.location {
            self.rail.awaiting.clear();
            self.cancel_network(cx);
        }
        self.remember_folder(path, travel);
        self.browse.route.asked.clear();
        self.selection_reset();
        self.validate_marks(false);
        self.find_abandon(cx);
        let (filter, chip, hidden) = (self.chip_filter(), self.listing.chip, self.show_hidden());
        let line = self.browse.listing.update(cx, |view, _| {
            view.open(path);
            let first = view.window_size();
            let listing = view.listing_mut();
            listing.set_filter(filter, chip);
            listing.set_hidden(hidden);
            listing.request_line(first)
        });
        let line = self.listing_request(line, cx);
        self.rail.recent_read = None;
        if recent {
            self.read_recent(cx);
        } else if self.send_listing(&line) {
            self.browse.in_flight += 1;
        }
        if self.browse.route.leaf_parent != path {
            self.browse.route.leaf.clear();
        }
        self.sync_facts(cx);
    }

    pub fn move_cursor(&mut self, index: usize, cx: &mut Context<Self>) {
        let ask = self.browse.listing.update(cx, |view, _| {
            view.listing_mut().move_cursor(index);
            view.reveal();
            let (held, cursor) = (view.listing().held(), view.listing().cursor());
            let inside = cursor >= held.start && cursor < held.start + held.rows.len();
            let count = view.window_size();
            (!inside)
                .then(|| {
                    view.listing()
                        .window_request(cursor.saturating_sub(count / 2), count)
                })
                .flatten()
        });
        if let Some(line) = ask {
            self.send_listing(&line);
        }
        self.sync_facts(cx);
    }

    // Enter or a double click on a row: a folder opens; in a save its file names Filename; otherwise the file answers.
    pub fn activate_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let held = self.browse.listing.read(cx).listing().held();
        let Some(row) = index
            .checked_sub(held.start)
            .and_then(|at| held.rows.get(at))
            .cloned()
        else {
            return;
        };
        if row.dir {
            let path = flea::ui_logic_picker_listing::row_path(&self.listing.location, &row.name);
            self.open_folder(&path, cx);
            return;
        }
        if !self.name_from_row(index, cx) {
            self.move_cursor(index, cx);
            self.accept(cx);
        }
    }

    // In a save a file row names Filename with its stem selected; Enter, a double click and Space all land here.
    pub(super) fn name_from_row(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let Some(filename) = self.browse.filename.clone() else {
            return false;
        };
        let held = self.browse.listing.read(cx).listing().held();
        let Some(row) = index
            .checked_sub(held.start)
            .and_then(|at| held.rows.get(at))
            .filter(|row| !row.dir)
            .cloned()
        else {
            return false;
        };
        self.move_cursor(index, cx);
        filename.update(cx, |field, cx| field.set(&row.name, cx));
        self.save_name_edited(row.name, cx);
        self.browse.focus_filename = true;
        cx.notify();
        true
    }

    // Filename's text is the save name the answer probes; a change drops any review of the old name.
    pub(super) fn save_name_edited(&mut self, name: String, cx: &mut Context<Self>) {
        if self.listing.save_name == name {
            return;
        }
        self.listing.save_name = name;
        self.invalidate_save(cx);
    }

    // Escape clears a standing name filter before it refuses the request.
    pub(super) fn escape_listing(&mut self, cx: &mut Context<Self>) -> Escape {
        self.browse
            .listing
            .update(cx, |view, _| view.listing_mut().escape())
    }

    // The listing owner's facts, copied once per landed line so the answer reads them without a frame-time walk.
    pub(super) fn sync_facts(&mut self, cx: &mut Context<Self>) {
        let view = self.browse.listing.read(cx);
        let (facts, shown) = (view.facts(), view.shown().map(<[usize]>::len));
        let moved = facts.location != self.listing.location;
        let listing = &mut self.listing;
        listing.shown_total = shown.unwrap_or(facts.total);
        listing.location = facts.location;
        listing.total = facts.total;
        listing.held = facts.first;
        listing.names = facts.names;
        listing.cursor = facts.cursor;
        listing.state = facts.state;
        listing.failed = facts.failed;
        listing.cursor_file = facts.cursor_file;
        if self.listing.failed {
            self.navigation_failed();
        }
        if moved {
            self.sync_path(cx);
            if let Some(filename) = &self.browse.filename {
                let folder = self.listing.location.clone();
                filename.update(cx, |field, cx| {
                    if let Some(save) = &field.save {
                        save.update(cx, |save, _| save.field().set_folder(&folder));
                    }
                });
            }
            if logic::saving(&self.req) {
                self.invalidate_save(cx);
            }
        } else if logic::saving(&self.req)
            && matches!(self.listing.state, "ready" | "empty")
            && self.review.is_none()
            && self.save_error.is_empty()
        {
            // The start folder lands without a move, so the supplied name is probed here, even with no rows (#428).
            self.probe_save();
        }
        cx.notify();
    }

    // The shared path entry draws and resolves against the folder the listing stands on.
    pub(super) fn sync_path(&mut self, cx: &mut Context<Self>) {
        let inputs = Inputs {
            paths: [self.listing.location.clone().into(), Default::default()],
            home: self.browse.home.clone().into(),
            failed: self.listing.failed,
            show_hidden: self.show_hidden(),
            first: FIRST,
            aliases: self.recent_alias(),
            elide_start: true,
            ..Inputs::default()
        };
        self.browse.path.update(cx, |bar, cx| bar.sync(inputs, cx));
    }
}
