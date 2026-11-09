//! The listing, background, Trash and Places menus built from one context, in inventory order.
use super::entry::{Brand, Entry, Id, Text};
use super::inventory::{Spec, INVENTORY};
use super::{flyouts, rows};

pub const S_IFMT: u32 = 0o170000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFLNK: u32 = 0o120000;
pub const ANY_EXECUTE_BIT: u32 = 0o111;
// The background offers these whatever Settings > Menus says; there the switches hide them from the file menu alone.
const BACKGROUND_KEEPS: [&str; 3] = ["openTerminal", "toggleHidden", "toggleLangMarks"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Background,
    Trash,
    Place,
    Rail,
}

impl Kind {
    fn letter(self) -> char {
        match self {
            Self::File => 'F',
            Self::Background => 'B',
            Self::Trash => 'T',
            Self::Place => 'P',
            Self::Rail => 'R',
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Peer {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct App {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub default: bool,
}

/// What a rail row is, read off the rail's own entry and nothing else.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RailEntry {
    pub group: String,
    pub kind: String,
    pub mounted: bool,
    pub locked: bool,
    pub removable: bool,
    pub loop_device: bool,
    pub read_only: bool,
    pub device: String,
    // True only when the saved place cannot be edited.
    pub fixed: bool,
}

/// The storage classes' thumbnail switches, as ui.json preview keeps them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThumbPrefs {
    pub network: bool,
    pub phone: bool,
    pub usb: bool,
}

impl Default for ThumbPrefs {
    fn default() -> Self {
        Self {
            network: false,
            phone: false,
            usb: true,
        }
    }
}

/// Everything a menu open reads; the owner fills it once at open and again only to validate a choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub has_row: bool,
    pub selection_count: usize,
    pub row_mode: u32,
    pub row_is_file: bool,
    pub row_is_symlink: bool,
    pub row_is_image: bool,
    // A PDF or an office document, which Convert takes alone.
    pub row_is_document: bool,
    pub row_is_archive: bool,
    pub row_name: String,
    pub archive_name: String,
    // Issue 397: every marked name when several are marked and each is an archive file, else empty.
    pub archive_names: Vec<String>,
    pub flash_zip_member: String,
    pub selection_modes: Vec<u32>,
    // Each marked item's own type bits, None while the held rows cannot answer.
    pub compare_kinds: Option<Vec<u32>>,
    // How many marked items are images, for Convert and Combine Images over several.
    pub selection_images: usize,
    // Whether every marked item is a kind that prints faithfully.
    pub printable: bool,
    pub has_shebang: bool,
    pub cursor_is_target: bool,
    pub clipboard_available: bool,
    pub clipboard_image_available: bool,
    pub clipboard_watch_failed: bool,
    pub can_trash: bool,
    pub can_link: bool,
    pub dir_writable: bool,
    pub archive_formats: Vec<String>,
    pub can_extract: bool,
    pub can_convert: bool,
    pub tag_submenu: Option<Vec<Entry>>,
    pub open_with_apps: Vec<App>,
    pub open_with_loaded: bool,
    // The catalogue missed the menu's deadline, so an empty list says nothing about the file's applications.
    pub open_with_late: bool,
    pub taildrop_installed: bool,
    pub taildrop_peers: Vec<Peer>,
    pub taildrop_refreshing: bool,
    pub taildrop_code: String,
    pub localsend_installed: bool,
    pub localsend_peers: Vec<Peer>,
    pub localsend_checking: bool,
    pub rclone_remotes: Vec<String>,
    pub dropbox_installed: bool,
    pub dropbox_path: String,
    pub dropbox_refreshing: bool,
    pub dropbox_code: String,
    pub row_in_dropbox: bool,
    // The AirDrop row: owly there, its plugin on and the Wi-Fi able; on once the radio is up; each device with its phone mark.
    pub airdrop_installed: bool,
    pub airdrop_on: bool,
    pub airdrop_devices: Vec<(Peer, bool)>,
    pub scripts: Vec<Peer>,
    pub hidden: Vec<String>,
    pub show_hidden: bool,
    pub lang_marks: bool,
    pub storage_class: String,
    pub thumbs: ThumbPrefs,
    pub update_version: String,
    pub has_folder_sort: bool,
    pub sort_by: String,
    pub place_favourite: bool,
    pub trash_total: i64,
    pub busy: bool,
    // The listing is a history such as Recent, with no folder of its own to read a relative path from.
    pub recent: bool,
    pub rail: Option<RailEntry>,
}

#[path = "build_context_default.rs"]
mod context_default;

pub fn listing_entries(p: &Context) -> Vec<Entry> {
    build_entries(
        if p.has_row {
            Kind::File
        } else {
            Kind::Background
        },
        p,
    )
}

// The Places rail's own, behind its Extras switch: off, the rail keeps the menu it has today.
pub fn place_entries(p: &Context) -> Vec<Entry> {
    if rows::is_hidden(&p.hidden, "placeMenu") {
        return Vec::new();
    }
    build_entries(Kind::Place, p)
}

pub fn trash_entries(total: i64, busy: bool) -> Vec<Entry> {
    build_entries(
        Kind::Trash,
        &Context {
            trash_total: total,
            busy,
            ..Context::default()
        },
    )
}

pub fn rail_entries(entry: &RailEntry) -> Vec<Entry> {
    build_entries(
        Kind::Rail,
        &Context {
            rail: Some(entry.clone()),
            ..Context::default()
        },
    )
}

pub fn build_entries(kind: Kind, p: &Context) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    let mut group = "";
    for spec in INVENTORY {
        if !spec.kinds.contains(kind.letter()) {
            continue;
        }
        // Rail rows are never hideable.
        if kind != Kind::Rail
            && rows::is_hidden(&p.hidden, spec.id)
            && !(kind == Kind::Background && BACKGROUND_KEEPS.contains(&spec.id))
        {
            continue;
        }
        let mut entry = Entry::row(spec.id, Text::Key(spec.label), spec.glyph);
        entry.action = Id::Borrowed(spec.action);
        if !available(&mut entry, spec, p, kind) {
            continue;
        }
        if !out.is_empty() && group != spec.group {
            out.push(Entry::separator());
        }
        group = spec.group;
        out.push(entry);
    }
    out
}

fn count_label(e: &mut Entry, count: usize, many: &'static str) {
    if count > 1 {
        e.label = Some(Text::Count(many, count as i64));
    }
}

fn provider(e: &mut Entry, brand: Brand, peers: &[Peer], reading: bool) {
    e.brand = Some(brand);
    e.glyph = None;
    e.submenu = Some(
        peers
            .iter()
            .map(|peer| {
                let mut entry = Entry::leaf(peer.id.clone(), Text::Data(peer.label.clone()));
                if brand == Brand::Tailscale {
                    entry.glyph = Some("server");
                }
                entry
            })
            .collect(),
    );
    e.disabled = reading || peers.is_empty();
    e.pending = reading;
    // A provider that cannot answer reads red with no state word, so the menu keeps its size (GM 2026-10-29).
    e.errored = e.disabled && !reading;
}

fn available(e: &mut Entry, spec: &Spec, p: &Context, kind: Kind) -> bool {
    if kind == Kind::Rail {
        return p
            .rail
            .as_ref()
            .is_some_and(|rail| rows::available_rail(spec.id, rail));
    }
    let count = p.selection_count;
    let action = spec.action;
    if kind == Kind::Place && (action == "addFavourite" || action == "removeFavourite") {
        return (action == "removeFavourite") == p.place_favourite;
    }
    let is_dir = p.row_mode & S_IFMT == S_IFDIR;
    // Only a folder can be a favourite, so a file or a wider selection hides the row rather than dimming it.
    if action == "addFavourite" && kind == Kind::File && (count != 1 || !is_dir) {
        return false;
    }
    if action == "paste" {
        e.disabled = !p.clipboard_available
            && !p.clipboard_image_available
            && !p.clipboard_watch_failed;
    }
    if action == "showOriginal" && !p.row_is_symlink {
        return false;
    }
    // Paste As shows only while files are on the clipboard and the filesystem holds links.
    if action == "pasteAs" {
        if !p.can_link || !p.clipboard_available {
            return false;
        }
        e.submenu = Some(rows::paste_as_entries());
    }
    if action == "copyAs" {
        if kind == Kind::Place {
            e.id = Id::Borrowed("copypath");
            e.action = Id::Borrowed("copypath");
            e.label = Some(Text::Key("menu.row.copy_path"));
        } else {
            e.submenu = Some(rows::flyout_leaves("copyAs", p.recent));
        }
    }
    if (action == "run" || action == "openAsText") && (count != 1 || is_dir) {
        return false;
    }
    if action == "print" {
        if !p.printable || count == 0 {
            return false;
        }
        count_label(e, count, "menu.row.print_many");
    }
    // Duplicate acts on one item, so a wider selection hides it.
    if action == "duplicate" && count != 1 {
        return false;
    }
    if action == "properties" {
        e.disabled = count != 1;
    }
    if action == "rename" {
        e.disabled = count < 1;
        count_label(e, count, "menu.row.rename_many");
    }
    if action == "permissions" {
        let permission = rows::permissions_entry(p.row_mode, count, &p.selection_modes);
        e.disabled = permission.disabled;
        e.errored = permission.errored;
        e.hint = permission.hint;
    }
    if action == "makeExecutable"
        && !rows::can_make_executable(p.row_mode, count, p.has_shebang, p.cursor_is_target)
    {
        return false;
    }
    if action == "compare" && !rows::can_compare(p.compare_kinds.as_deref()) {
        return false;
    }
    if action == "runScript" {
        if p.scripts.is_empty() {
            return false;
        }
        e.submenu = Some(
            p.scripts
                .iter()
                .map(|s| Entry::leaf(s.id.clone(), Text::Data(s.label.clone())))
                .collect(),
        );
    }
    if action == "compress" {
        if p.archive_formats.is_empty() {
            return false;
        }
        e.submenu = Some(rows::compress_entries(&p.archive_formats));
    }
    if action == "trash" && !p.can_trash {
        return false;
    }
    if !p.dir_writable
        && [
            "newFolder",
            "newFile",
            "paste",
            "pasteAs",
            "duplicate",
            "rename",
            "trash",
            "deletePermanently",
        ]
        .contains(&action)
    {
        e.disabled = true;
    }
    if action == "tags" {
        // A read of the selection's tag states replaces these leaves where its owner supplies them.
        e.submenu = Some(p.tag_submenu.clone().unwrap_or_else(flyouts::tag_entries));
    }
    if action == "extract" && !rows::extract_entry(e, p, count) {
        return false;
    }
    // Convert takes one image, one PDF, one office document or a selection of images only.
    if action == "convert"
        && !(p.can_convert
            && count >= 1
            && (if count == 1 {
                p.row_is_image || p.row_is_document
            } else {
                p.selection_images == count
            }))
    {
        return false;
    }
    if action == "combineImages" && !(p.can_convert && count >= 2 && p.selection_images == count) {
        return false;
    }
    if action == "flashDrive"
        && (count != 1 || !rows::is_disk_image(&p.row_name, &p.flash_zip_member))
    {
        return false;
    }
    if action == "openWith" {
        if count == 1 && p.open_with_loaded && !p.open_with_late && p.open_with_apps.is_empty() {
            return false;
        }
        e.submenu = Some(flyouts::open_with_entries(&p.open_with_apps, count == 1));
        e.disabled = count == 0 || (count > 1 && p.open_with_apps.is_empty());
    }
    if action == "sendRclone" {
        if p.rclone_remotes.is_empty() {
            return false;
        }
        e.submenu = Some(flyouts::rclone_entries(&p.rclone_remotes));
    }
    if action == "taildrop" {
        if !p.taildrop_installed {
            return false;
        }
        // A folder takes no Taildrop peer, as v0.3.12 MenuContext.js empties them off a file row.
        let peers: &[Peer] = if p.row_is_file {
            &p.taildrop_peers
        } else {
            &[]
        };
        provider(e, Brand::Tailscale, peers, p.taildrop_refreshing);
        // Menu.js submenuGlyph: a Taildrop peer is a machine, so its row draws the server mark.
        for leaf in e.submenu.iter_mut().flatten() {
            leaf.glyph = Some(rows::submenu_glyph(action));
        }
    }
    if action == "airdrop" {
        if !p.airdrop_installed {
            return false;
        }
        e.brand = Some(Brand::AirDrop);
        e.glyph = None;
        e.submenu = Some(super::airdrop::flyout(p.airdrop_on, &p.airdrop_devices));
    }
    if action == "localsend" {
        if !p.localsend_installed {
            return false;
        }
        provider(
            e,
            Brand::LocalSend,
            &p.localsend_peers,
            p.localsend_checking,
        );
    }
    if action == "dropbox" || action == "sharelink" {
        if !p.dropbox_installed
            || (if action == "dropbox" {
                p.row_in_dropbox
            } else {
                !p.row_in_dropbox
            })
        {
            return false;
        }
        e.disabled = p.dropbox_refreshing || p.dropbox_path.is_empty();
        if action == "dropbox" {
            e.brand = Some(Brand::Dropbox);
            e.glyph = None;
        }
        e.pending = p.dropbox_refreshing;
        e.errored = e.disabled && !p.dropbox_refreshing;
    }
    if action == "sort" {
        e.submenu = Some(rows::sort_entries(p.has_folder_sort, &p.sort_by));
    }
    if action == "updateFlea" {
        if p.update_version.is_empty() {
            return false;
        }
        e.hint = Some(Text::Data(p.update_version.clone()));
        e.hint_square = true;
    }
    if action == "toggleHidden" {
        let (label, glyph) = rows::hidden_row(p.show_hidden);
        e.label = Some(Text::Key(label));
        e.glyph = Some(glyph);
    }
    if action == "toggleLangMarks" {
        e.label = Some(Text::Key(rows::code_icons_label(p.lang_marks)));
    }
    if action == "extThumbs" {
        return rows::ext_thumbs_entry(e, &p.storage_class, p.thumbs);
    }
    if action == "restoreAll" || action == "emptyTrash" {
        e.disabled = p.trash_total <= 0 || p.busy;
    }
    if ["trash", "deletePermanently", "emptyTrash"].contains(&action) {
        e.danger = true;
    }
    true
}
