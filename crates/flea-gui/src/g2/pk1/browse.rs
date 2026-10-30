//! The chooser's folder on its listing child: generation-matched replies, the typed-path route and row activation.
use super::filename::Filename;
use super::picker::Picker;
use crate::g2::pk2::chrome::PathBar;
use crate::g2::pk2::view::{ListingView, SaveView};
use crate::g2::pk2::{Escape, SaveField};
use crate::list::Typeface;
use crate::path::view::{Inputs, Out, PathView};
use flea::jsondoc::{self, Json};
use flea::ui_logic_pk1 as logic;
use flea::ui_logic_pk2::FilterDef;
use gpui::{prelude::*, AnyView, AnyWindowHandle, Context, Entity, Subscription};

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
    reply_gen: u64,
    replied: bool,
    pub(super) route: Route,
    pub(super) window: Option<AnyWindowHandle>,
    pub(super) focus_filename: bool,
    pub(super) stems_seen: u64,
    _path: Subscription,
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
        let path = cx.new(|cx| PathBar::new(catalog, cx));
        let view = path.read(cx).path();
        let subscription = cx.subscribe(&view, |picker: &mut Picker, _, out: &Out, cx| {
            picker.path_out(out.clone(), cx)
        });
        let filename = logic::saving(req).then(|| {
            let seed = logic::initial_save_name(req).to_owned();
            let save = cx.new(|_| SaveView::new(catalog.clone(), SaveField::new(&seed)));
            let label = catalog.tr("picker.browser.save_as");
            cx.new(|cx| Filename::new(&seed, label, save, cx))
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
        }
    }

    pub fn body(&self, cx: &mut Context<Picker>) -> AnyView {
        let (path, listing) = (self.path.clone(), self.listing.clone());
        let picker = cx.entity().downgrade();
        let rail = cx.new(|_| super::rail_view::RailView {
            picker: picker.clone(),
        });
        let path = cx.new(|_| super::chrome_view::ChromeView {
            picker: picker.clone(),
            path,
        });
        AnyView::from(cx.new(|_| super::body::Body {
            path,
            rail,
            listing,
            picker,
        }))
    }

    pub fn path_view(&self, cx: &gpui::App) -> Entity<PathView> {
        self.path.read(cx).path()
    }
}

impl Picker {
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
        let recent = logic::is_recent(path);
        if !path.starts_with('/') && !recent {
            return;
        }
        // A folder change ends any drive wait, as v0.3.12 PickerPlaces.qml onPathChanged does.
        if path != self.listing.location {
            self.rail.awaiting.clear();
        }
        self.remember_folder(path);
        self.selection_reset();
        self.validate_marks(false);
        self.find_abandon(cx);
        let (filter, chip, hidden) = (
            self.chip_filter(),
            self.listing.chip,
            self.req.hidden.unwrap_or(false),
        );
        let line = self.browse.listing.update(cx, |view, _| {
            view.open(path);
            let first = view.window_size();
            let listing = view.listing_mut();
            listing.set_filter(filter, chip);
            listing.set_hidden(hidden);
            listing.request_line(first)
        });
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
            let path = flea::ui_logic_pk2::row_path(&self.listing.location, &row.name);
            self.open_folder(&path, cx);
            return;
        }
        self.move_cursor(index, cx);
        if let Some(filename) = self.browse.filename.clone() {
            filename.update(cx, |field, cx| field.set(&row.name, cx));
            self.save_name_edited(row.name.clone(), cx);
            self.browse.focus_filename = true;
            cx.notify();
            return;
        }
        self.accept(cx);
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
        if moved {
            self.sync_path(cx);
            if let Some(filename) = &self.browse.filename {
                let folder = self.listing.location.clone();
                filename.update(cx, |field, cx| {
                    field
                        .save
                        .update(cx, |save, _| save.field().set_folder(&folder))
                });
            }
            if logic::saving(&self.req) {
                self.invalidate_save(cx);
            }
        }
        cx.notify();
    }

    // The shared path entry draws and resolves against the folder the listing stands on.
    pub(super) fn sync_path(&mut self, cx: &mut Context<Self>) {
        let inputs = Inputs {
            paths: [self.listing.location.clone().into(), Default::default()],
            home: self.browse.home.clone().into(),
            failed: self.listing.failed,
            show_hidden: self.req.hidden.unwrap_or(false),
            first: FIRST,
            aliases: self.recent_alias(),
            ..Inputs::default()
        };
        self.browse.path.update(cx, |bar, cx| bar.sync(inputs, cx));
    }

    // One whole line from the listing child.
    pub(super) fn listing_line(&mut self, text: &str, cx: &mut Context<Self>) {
        self.browse.replied = true;
        // A line that names no answer cannot be stale, so it fails the folder the window stands on.
        let Ok(document) = jsondoc::parse(text) else {
            self.browse.listing.update(cx, |view, _| {
                let generation = view.generation();
                view.receive(generation, text)
            });
            return self.sync_facts(cx);
        };
        let kind = document
            .get("t")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned();
        match kind.as_str() {
            "jumped" => return self.jump_answer(&document, cx),
            "paths" => return self.selection_paths(&document, cx),
            "filtered" => return self.find_line(text, cx),
            "peeked" => return self.peeked(&document, cx),
            "located" => return self.located(&document, cx),
            "listed" => self.landing(&document, cx),
            "error" if document.get("where").and_then(Json::as_str) == Some("scan") => {
                self.landing(&document, cx)
            }
            _ => {}
        }
        let generation = self.browse.reply_gen;
        let taken = self
            .browse
            .listing
            .update(cx, |view, _| view.receive(generation, text));
        self.sync_facts(cx);
        if taken && kind == "listed" {
            self.relocate();
        }
        if taken {
            if let Some(stamp) = (kind == "rows")
                .then(|| flea::json::field_usize(text, "listing"))
                .flatten()
            {
                self.find_stamp(stamp, cx);
            }
            self.take_leaf(cx);
            if kind == "rows" {
                self.refill_grid(cx);
            }
        }
    }

    // A listed or scan answer closes one request; it is current only when it answers the newest for the drawn folder.
    fn landing(&mut self, document: &Json, cx: &mut Context<Self>) {
        self.browse.in_flight = self.browse.in_flight.saturating_sub(1);
        let view = self.browse.listing.read(cx);
        // Recent's listing stands on the base "/" its paths are named under.
        let listed = if logic::is_recent(&self.listing.location) {
            "/"
        } else {
            self.listing.location.as_str()
        };
        let current = view.listing().is_pending()
            && self.browse.in_flight == 0
            && document.get("path").and_then(Json::as_str) == Some(listed);
        self.browse.reply_gen = if current { view.generation() } else { 0 };
    }

    // The listing child left: rows already drawn stay, and the footer says which failure it was.
    pub(super) fn listing_failed(&mut self, cx: &mut Context<Self>) {
        let cause = if self.browse.replied {
            crate::g2::pk2::Cause::Gone
        } else {
            crate::g2::pk2::Cause::Unstarted
        };
        self.browse.listing.update(cx, |view, _| {
            let generation = view.generation();
            view.fail(generation, cause)
        });
        self.sync_facts(cx);
    }
}
