//! The rows and flyouts one row opens, the hidden set's reader, and the rules a row's state follows.
use super::build::{
    Context, RailEntry, ThumbPrefs, ANY_EXECUTE_BIT, S_IFDIR, S_IFLNK, S_IFMT, S_IFREG,
};
use super::entry::{Entry, Id, Text};

// The flyout's tail row, the only way into the Open With dialog.
pub const OPEN_WITH_OTHER: &str = "__another__";
// The Sort By flyout's forget row; the folder sort's forget branch reads this spelling.
pub const SORT_DEFAULT: &str = "__default__";
// The backend's own orders, so the flyout never offers one it would refuse.
pub const SORT_ORDERS: [(&str, &str); 4] = [
    ("name", "menu.sort.name"),
    ("size", "menu.sort.size"),
    ("mtime", "menu.sort.modified"),
    ("kind", "menu.sort.kind"),
];

// Open is never in the stored set, so a hand-edited state file cannot empty a menu.
pub fn is_hidden(hidden: &[String], action: &str) -> bool {
    if action == "open" {
        return false;
    }
    hidden.iter().any(|id| id == action)
}

// A hidden row leaves, and a separator that lost every row it divided leaves with it; order never changes.
pub fn apply_hidden(entries: &[Entry], hidden: &[String]) -> Vec<Entry> {
    let mut kept: Vec<Entry> = Vec::new();
    for entry in entries {
        if entry.separator {
            if kept.last().is_some_and(|last| !last.separator) {
                kept.push(entry.clone());
            }
            continue;
        }
        let id = if entry.id.is_empty() {
            &entry.action
        } else {
            &entry.id
        };
        if !is_hidden(hidden, id) {
            kept.push(entry.clone());
        }
    }
    while kept.last().is_some_and(|last| last.separator) {
        kept.pop();
    }
    kept
}

// Rail rows read the rail entry alone; Mount and Unmount never meet, and Open is the row's own click.
pub fn available_rail(id: &str, entry: &RailEntry) -> bool {
    let device = entry.group == "device";
    let volume = device && entry.kind == "volume";
    let phone = device && entry.kind == "phone";
    let share = entry.group == "network" && entry.kind == "share";
    let mounted = entry.mounted;
    match id {
        "unlockVolume" => volume && entry.locked,
        "mountVolume" => volume && !mounted && !entry.locked,
        "mountPhone" => phone && !mounted,
        "open" => (volume || phone || share) && mounted,
        "unmountVolume" => volume && mounted,
        "unmountPhone" => phone && mounted,
        "unmount" => share && mounted,
        "eject" => volume && mounted && (entry.removable || entry.loop_device),
        // Never on an open crypt, whose mapper names no drive.
        "flashImage" => {
            volume
                && entry.removable
                && !entry.read_only
                && !entry.device.starts_with("/dev/mapper/")
                && !entry.device.starts_with("/dev/dm-")
        }
        "rename" | "editPlace" | "remove" => share && !entry.fixed,
        "removeFavourite" => entry.kind == "favourite",
        _ => false,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Refusal {
    pub disabled: bool,
    pub errored: bool,
    pub hint: Option<Text>,
}

// A link names itself; any other kind that is neither a file nor a folder is special, and an unread mode names nothing.
pub fn permissions_word(mode: u32) -> Option<Text> {
    match mode & S_IFMT {
        0 => None,
        S_IFLNK => Some(Text::Key("menu.state.symlink")),
        _ => Some(Text::Key("menu.state.special_file")),
    }
}

fn plain_kind(mode: u32) -> bool {
    matches!(mode & S_IFMT, S_IFREG | S_IFDIR)
}

// The mode describes the selected object itself, so a symlink never grants access to its unseen target.
pub fn permissions_entry(mode: u32, count: usize, modes: &[u32]) -> Refusal {
    if !modes.is_empty() {
        return match modes.iter().find(|mode| !plain_kind(**mode)) {
            Some(bad) => Refusal {
                disabled: true,
                errored: true,
                hint: permissions_word(*bad),
            },
            None => Refusal::default(),
        };
    }
    let allowed = count >= 1 && plain_kind(mode);
    // No selection refuses for want of an item, so it names no kind.
    let hint = if !allowed && count >= 1 {
        permissions_word(mode)
    } else {
        None
    };
    Refusal {
        disabled: !allowed,
        errored: !allowed,
        hint,
    }
}

// Only a single cursor-row regular file with a shebang and no execute bit at all.
pub fn can_make_executable(
    mode: u32,
    count: usize,
    has_shebang: bool,
    cursor_is_target: bool,
) -> bool {
    count == 1
        && has_shebang
        && cursor_is_target
        && mode & S_IFMT == S_IFREG
        && mode & ANY_EXECUTE_BIT == 0
}

// Exactly two marked regular files; links stay links.
pub fn can_compare(kinds: Option<&[u32]>) -> bool {
    kinds.is_some_and(|kinds| kinds.len() == 2 && kinds.iter().all(|kind| kind & S_IFMT == S_IFREG))
}

// The Copy As flyout: seven leaves in board order, letters on the key hint slot.
pub fn copy_as_entries() -> Vec<Entry> {
    vec![
        Entry::leaf("copyPath", Text::Key("menu.copy_as.path"))
            .with_glyph("file-text")
            .with_key_hint("p"),
        Entry::leaf("copyRelative", Text::Key("menu.copy_as.relative"))
            .with_glyph("file")
            .with_key_hint("r"),
        Entry::leaf("copyName", Text::Key("menu.copy_as.name"))
            .with_glyph("type")
            .with_key_hint("n"),
        Entry::leaf("copyStem", Text::Key("menu.copy_as.stem"))
            .with_glyph("type")
            .with_key_hint("e"),
        Entry::leaf("copydirpath", Text::Key("menu.copy_as.folder"))
            .with_glyph("folder")
            .with_key_hint("f"),
        Entry::leaf("copyUri", Text::Key("menu.copy_as.uri"))
            .with_glyph("globe")
            .with_key_hint("u"),
        Entry::leaf("copyQuoted", Text::Key("menu.copy_as.quoted"))
            .with_glyph("terminal")
            .with_key_hint("s"),
    ]
}

// l and h open and close a flyout, so the Paste As letters are L, a and H.
pub fn paste_as_entries() -> Vec<Entry> {
    vec![
        Entry::leaf("pasteLink", Text::Key("menu.paste_as.link"))
            .with_glyph("symlink")
            .with_key_hint("L"),
        Entry::leaf("pasteAbsoluteLink", Text::Key("menu.paste_as.absolute"))
            .with_glyph("symlink")
            .with_key_hint("a"),
        Entry::leaf("pasteHardLink", Text::Key("menu.paste_as.hard"))
            .with_glyph("copy")
            .with_key_hint("H"),
    ]
}

// A hidden row still opens its flyout, built from the action.
pub fn flyout_entries(action: &str) -> Vec<Entry> {
    match action {
        "copyAs" => copy_as_entries(),
        "pasteAs" => paste_as_entries(),
        _ => Vec::new(),
    }
}

// A history listing such as Recent has no folder of its own, so Relative Path hides there as a row that cannot act does.
pub fn flyout_leaves(action: &str, history: bool) -> Vec<Entry> {
    let mut leaves = flyout_entries(action);
    if history {
        leaves.retain(|leaf| leaf.id != "copyRelative");
    }
    leaves
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Submenu {
    None,
    Refuse,
    Row(usize),
    Lone,
}

// Which flyout c or P opens: the drawn row's, a lone one for a hidden row, or a refusal.
pub fn submenu_for(action: &str, entries: &[Entry], clipboard_available: bool) -> Submenu {
    // A Tags row carries its own leaves in tag_submenu, so it has no fixed flyout list here.
    if flyout_entries(action).is_empty() && action != "tags" {
        return Submenu::None;
    }
    if let Some(index) = entries.iter().position(|entry| entry.action == action) {
        return if entries[index].disabled || !entries[index].has_submenu() {
            Submenu::Refuse
        } else {
            Submenu::Row(index)
        };
    }
    if action == "pasteAs" && !clipboard_available {
        return Submenu::Refuse;
    }
    if action == "tags" {
        return Submenu::None;
    }
    Submenu::Lone
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lone {
    Moved,
    Unknown,
    Fire(String),
}

// A moved selection outranks an unknown leaf; a rail or header menu has no selection to move.
pub fn lone_choice(
    action: &str,
    id: &str,
    for_rail: bool,
    for_header: bool,
    has_row: bool,
    opened: &str,
    now: &str,
) -> Lone {
    let known = flyout_entries(action)
        .iter()
        .any(|leaf| !leaf.separator && leaf.id == id);
    if !for_rail && !for_header && has_row && opened != now {
        return Lone::Moved;
    }
    if !known {
        return Lone::Unknown;
    }
    Lone::Fire(format!("{action}:{id}"))
}

// Sort By marks the order in effect with a checkmark rather than repeating the sort mark on every row.
pub fn sort_entries(has_own: bool, current: &str) -> Vec<Entry> {
    let mut out: Vec<Entry> = SORT_ORDERS
        .iter()
        .map(|(id, key)| Entry {
            check: Some(*id == current),
            ..Entry::leaf(*id, Text::Key(key))
        })
        .collect();
    if has_own {
        out.push(Entry::separator());
        out.push(Entry {
            check: Some(false),
            ..Entry::leaf(SORT_DEFAULT, Text::Key("menu.sort.default"))
        });
    }
    out
}

// The one mark every row of an open flyout draws unless the row carries its own.
pub fn submenu_glyph(action: &str) -> &'static str {
    match action {
        "taildrop" => "server",
        "runScript" => "terminal",
        "sort" => "sort",
        _ => "archive",
    }
}

pub fn hidden_row(show_hidden: bool) -> (&'static str, &'static str) {
    if show_hidden {
        ("menu.row.hide_hidden", "eye-off")
    } else {
        ("menu.row.show_hidden", "eye")
    }
}

pub fn code_icons_label(lang_marks: bool) -> &'static str {
    if lang_marks {
        "menu.row.hide_code_icons"
    } else {
        "menu.row.show_code_icons"
    }
}

// The column header's rows; Name is never offered, a hidden column reads Show and a drawn one Hide.
pub fn header_entries(hidden_cols: &[String], show_hidden: bool) -> Vec<Entry> {
    const COLUMNS: [(&str, &str, &str, &str); 4] = [
        (
            "mode",
            "menu.header.show_mode",
            "menu.header.hide_mode",
            "lock",
        ),
        (
            "size",
            "menu.header.show_size",
            "menu.header.hide_size",
            "drive",
        ),
        (
            "date",
            "menu.header.show_date",
            "menu.header.hide_date",
            "download",
        ),
        (
            "kind",
            "menu.header.show_kind",
            "menu.header.hide_kind",
            "type",
        ),
    ];
    let mut out: Vec<Entry> = COLUMNS
        .iter()
        .map(|(key, show, hide, glyph)| {
            let shown = !hidden_cols.iter().any(|col| col == key);
            let mut entry = Entry::row(
                Id::Owned(format!("col:{key}")),
                Text::Key(if shown { hide } else { show }),
                glyph,
            );
            entry.id = Id::Borrowed("");
            entry
        })
        .collect();
    out.push(Entry::separator());
    let (label, glyph) = hidden_row(show_hidden);
    out.push(Entry {
        id: Id::Borrowed(""),
        ..Entry::row("toggleHidden", Text::Key(label), glyph)
    });
    out
}

// The probed formats only, then the password row where 7z is present, since bsdtar writes no encryption.
pub fn compress_entries(formats: &[String]) -> Vec<Entry> {
    let mut out: Vec<Entry> = formats
        .iter()
        .map(|f| Entry::leaf(f.clone(), Text::Data(format!(".{f}"))).with_glyph("file"))
        .collect();
    if formats.iter().any(|f| f == "7z") {
        out.push(Entry::separator());
        out.push(
            Entry::leaf("compressPassword", Text::Key("menu.compress.password")).with_glyph("key"),
        );
    }
    out
}

fn ends_with(lower: &str, suffix: &str) -> bool {
    lower.len() >= suffix.len() && lower.ends_with(suffix)
}

// A disk image by name, longest form first so .img.xz wins over .img; a zip needs one such member.
pub fn image_kind(name: &str) -> &'static str {
    const TABLE: [(&str, &str); 8] = [
        (".img.xz", "xz"),
        (".img.gz", "gz"),
        (".img.zst", "zst"),
        (".img.bz2", "bz2"),
        (".iso", "iso"),
        (".img", "img"),
        (".raw", "raw"),
        (".zip", "zip"),
    ];
    let lower = name.to_lowercase();
    TABLE
        .iter()
        .find(|(suffix, _)| lower.len() > suffix.len() && lower.ends_with(suffix))
        .map_or("", |(_, kind)| kind)
}

pub fn is_disk_image(name: &str, zip_member: &str) -> bool {
    match image_kind(name) {
        "" => false,
        "zip" => !matches!(image_kind(zip_member), "" | "zip"),
        _ => true,
    }
}

// The missing program a disabled Extract names: only bsdtar offers tar and only 7z offers 7z.
pub fn extract_hint(formats: &[String]) -> &'static str {
    if formats.iter().any(|f| f == "7z") {
        "menu.extract.no_bsdtar"
    } else if formats.iter().any(|f| f == "tar") {
        "menu.extract.no_7zip"
    } else {
        "menu.extract.no_tool"
    }
}

// Absent for none and for a disk image, which lists and never extracts; disabled with its reason for no reader.
// Issue 397: a selection made only of archives runs one extract per archive, so it is offered for several too.
pub fn extract_entry(entry: &mut Entry, p: &Context, count: usize) -> bool {
    let several = count > 1 && p.archive_names.len() == count;
    if !(several || p.row_is_archive && count == 1) {
        return false;
    }
    let names = if several {
        &p.archive_names[..]
    } else {
        std::slice::from_ref(&p.archive_name)
    };
    if names
        .iter()
        .any(|name| ends_with(&name.to_lowercase(), ".iso"))
    {
        return false;
    }
    if !p.can_extract {
        entry.disabled = true;
        entry.hint = Some(Text::Key(extract_hint(&p.archive_formats)));
        entry.hint_wrap = true;
    }
    true
}

fn class_key(storage_class: &str) -> Option<&'static str> {
    match storage_class {
        "network" => Some("menu.class.network"),
        "phone" => Some("menu.class.phone"),
        "usb" => Some("menu.class.usb"),
        _ => None,
    }
}

pub fn class_on(storage_class: &str, thumbs: ThumbPrefs) -> bool {
    match storage_class {
        "network" => thumbs.network,
        "phone" => thumbs.phone,
        "usb" => thumbs.usb,
        _ => true,
    }
}

// Present only on network, phone or USB storage, labelled for the class and its state.
pub fn ext_thumbs_entry(entry: &mut Entry, storage_class: &str, thumbs: ThumbPrefs) -> bool {
    let Some(class) = class_key(storage_class) else {
        return false;
    };
    let template = if class_on(storage_class, thumbs) {
        "menu.row.hide_thumbnails"
    } else {
        "menu.row.show_thumbnails"
    };
    entry.label = Some(Text::KeyArg(template, class));
    true
}

// A separator, a disabled row or a busy row is never the cursor; a step skips one and an end keeps the cursor.
pub fn step_row(rows: &[Entry], from: isize, delta: isize) -> isize {
    let mut at = from + delta;
    while at >= 0 && (at as usize) < rows.len() {
        if rows[at as usize].steppable() {
            return at;
        }
        at += delta;
    }
    from
}

// Flip at the far edge first, then shift only when neither side fits.
pub fn clamp(point: f32, size: f32, bounds: f32) -> f32 {
    let start = if point + size <= bounds {
        point
    } else {
        point - size
    };
    start.min((bounds - size).max(0.)).max(0.)
}
