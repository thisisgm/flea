// The Properties card's facts: the single item's pairs in board order, a folder's walk, and the several-item summary.
use crate::i18n::I18n;
use crate::jsondoc::Json;
use crate::ui_logic_preview_core::{meta, property};

/// The single-item `properties` reply.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub path: String,
    pub kind: String,
    pub directory: bool,
    pub symlink: bool,
    pub target: String,
    pub bytes: u64,
    pub modified: i64,
    pub mode: String,
    pub owner: String,
    pub uid: u64,
    pub gid: u64,
    pub tags: Vec<String>,
    /// The photo object the reply read from the file's EXIF, kept as sent; None for null or a reply without one.
    pub photo: Option<Json>,
}

fn text(reply: &Json, key: &str) -> String {
    reply.get(key).and_then(Json::as_str).unwrap_or("").to_string()
}

fn number(reply: &Json, key: &str) -> f64 {
    reply.get(key).and_then(Json::as_f64).filter(|n| n.is_finite()).unwrap_or(0.)
}

fn flag(reply: &Json, key: &str) -> bool {
    reply.get(key).and_then(Json::as_bool) == Some(true)
}

impl Facts {
    /// A reply without a tags key reads as untagged instead of failing.
    pub fn from_reply(reply: &Json) -> Self {
        Self {
            path: text(reply, "path"),
            kind: text(reply, "kind"),
            directory: flag(reply, "directory"),
            symlink: flag(reply, "symlink"),
            target: text(reply, "target"),
            bytes: number(reply, "bytes").max(0.) as u64,
            modified: number(reply, "modified") as i64,
            mode: text(reply, "mode"),
            owner: text(reply, "owner"),
            uid: number(reply, "uid").max(0.) as u64,
            gid: number(reply, "gid").max(0.) as u64,
            tags: reply.get("tags").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(str::to_string).collect(),
            photo: reply.get("photo").filter(|photo| photo.as_object().is_some()).cloned(),
        }
    }

    /// The backend names a regular file "File", untranslated; only that kind has codecs or deferred photo facts, and the backend refuses the rest.
    pub fn asks_media(&self) -> bool {
        self.kind == "File"
    }
}

/// Sample input: {"op":"facts","id":9,"ok":true,"vcodec":"h264","acodec":"aac","bps":8100000,"make":"Canon","model":"Canon EOS R6","lens":"","shutter":[1,250],"fnumber":[28,10],"iso":400,"taken":"2026:09:12 18:31:00"}.
pub fn media(reply: &Json) -> property::Facts {
    property::Facts {
        ok: flag(reply, "ok") && !flag(reply, "cancelled"),
        vcodec: text(reply, "vcodec"),
        acodec: text(reply, "acodec"),
        bps: number(reply, "bps"),
        make: text(reply, "make"),
        model: text(reply, "model"),
        lens: text(reply, "lens"),
        shutter: meta::ratio(reply, "shutter"),
        fnumber: meta::ratio(reply, "fnumber"),
        iso: number(reply, "iso").max(0.) as u32,
        taken: text(reply, "taken"),
    }
}

/// Until the facts reply lands, or when it is refused, the properties reply's own photo speaks alone, as v0.3.12 did.
pub fn media_pairs(
    i18n: &I18n,
    facts: &Facts,
    media: Option<&property::Facts>,
) -> Vec<(String, String)> {
    if !facts.asks_media() {
        return Vec::new();
    }
    let photo = facts.photo.as_ref().and_then(meta::photo);
    let pending = property::Facts {
        ok: true,
        ..property::Facts::default()
    };
    property::extra_pairs(
        Some(media.filter(|media| media.ok).unwrap_or(&pending)),
        photo.as_ref(),
        i18n,
    )
}

/// A folder's walk as the card knows it: the floor while it counts, the whole once done.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Walk {
    pub bytes: u64,
    pub items: u64,
    pub partial: bool,
    pub done: bool,
}

/// A folder starts without counted content; its entry size is not a recursive total.
pub fn begin(_facts: &Facts) -> Walk {
    Walk::default()
}

/// A cancelled or refused walk ends as the floor already counted, so the card never counts forever.
pub fn next(walk: Walk, reply: &Json) -> Walk {
    if flag(reply, "cancelled") {
        return if walk.done { walk } else { Walk { partial: true, done: true, ..walk } };
    }
    if !flag(reply, "ok") {
        return Walk { partial: true, done: true, ..walk };
    }
    Walk { bytes: number(reply, "bytes").max(0.) as u64, items: number(reply, "items").max(0.) as u64, partial: flag(reply, "partial"), done: flag(reply, "done") }
}

/// One drawn value: the value in the row's ink and an optional muted tail on the same line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub value: String,
    pub tail: String,
}

fn floor(walk: Option<Walk>) -> bool {
    walk.is_some_and(|w| !w.done || w.partial)
}

fn tail(i18n: &I18n, counting: bool) -> String {
    if counting { i18n.tr("dialogs.file.properties.counting") } else { String::new() }
}

/// A file's own bytes or a folder's walk; incomplete zero is unknown, positive partial bytes remain a floor.
pub fn size(i18n: &I18n, bytes: u64, walk: Option<Walk>) -> Line {
    let shown = walk.map_or(bytes, |w| w.bytes);
    let key = if floor(walk) { "dialogs.file.properties.size_floor" } else { "dialogs.file.properties.size" };
    let locale = i18n.locale();
    Line {
        value: if shown == 0 && floor(walk) {
            i18n.tr("dialogs.file.properties.unknown")
        } else {
            i18n.tr_args(key, &[&locale.format_size(shown), &locale.format_count(shown as i64)])
        },
        tail: tail(i18n, walk.is_some_and(|w| !w.done)),
    }
}

pub fn contains(i18n: &I18n, walk: Option<Walk>) -> Option<Line> {
    let walk = walk?;
    let key = if floor(Some(walk)) { "dialogs.file.properties.contains_floor" } else { "items.count" };
    Some(Line { value: i18n.trn(key, walk.items as i64), tail: tail(i18n, !walk.done) })
}

/// A fact's name, the label column's word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Label {
    Path,
    Kind,
    Size,
    Contains,
    Tags,
    Modified,
    Permissions,
    Owner,
    Group,
    LinkTarget,
    Kinds,
    Combined,
    Newest,
    Oldest,
}

impl Label {
    pub fn key(self) -> &'static str {
        match self {
            Self::Path => "dialogs.file.properties.path",
            Self::Kind => "dialogs.file.properties.kind",
            Self::Size => "dialogs.file.properties.size_label",
            Self::Contains => "dialogs.file.properties.contains",
            Self::Tags => "dialogs.file.properties.tags",
            Self::Modified => "dialogs.file.properties.modified",
            Self::Permissions => "dialogs.file.properties.permissions",
            Self::Owner => "dialogs.file.properties.owner",
            Self::Group => "dialogs.file.properties.group",
            Self::LinkTarget => "dialogs.file.properties.link_target",
            Self::Kinds => "dialogs.file.properties.kinds",
            Self::Combined => "dialogs.file.properties.combined",
            Self::Newest => "dialogs.file.properties.newest",
            Self::Oldest => "dialogs.file.properties.oldest",
        }
    }
}

/// One pair's name: one of the card's own labels, or an extra pair the facts view already named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Name {
    Own(Label),
    Extra(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    pub name: Name,
    pub line: Line,
}

fn pair(label: Label, value: String) -> Pair {
    Pair { name: Name::Own(label), line: Line { value, tail: String::new() } }
}

/// The Kind value: Folder for a directory, the MIME database's word for a file when one is known.
pub fn kind_word(i18n: &I18n, facts: &Facts, mime_kind: &str) -> String {
    match facts.kind.as_str() {
        _ if facts.directory => i18n.tr("dialogs.file.properties.kind_folder"),
        "Symbolic link" => i18n.tr("dialogs.file.properties.kind_link"),
        "Special file" => i18n.tr("dialogs.file.properties.kind_special"),
        _ if !mime_kind.is_empty() => mime_kind.to_string(),
        _ => i18n.tr("dialogs.file.properties.kind_file"),
    }
}

/// Board order: Path, Kind, Size, Contains for a folder, the facts view's extras, Tags, Modified, Permissions, Owner, Group, Link target.
pub fn pairs(i18n: &I18n, facts: &Facts, walk: Option<Walk>, extras: &[(String, String)], mime_kind: &str) -> Vec<Pair> {
    let mut out = vec![pair(Label::Path, facts.path.clone()), pair(Label::Kind, kind_word(i18n, facts, mime_kind))];
    out.push(Pair { name: Name::Own(Label::Size), line: size(i18n, facts.bytes, walk) });
    if facts.directory {
        let line = contains(i18n, walk).unwrap_or_default();
        out.push(Pair { name: Name::Own(Label::Contains), line });
    }
    out.extend(extras.iter().map(|(name, value)| Pair { name: Name::Extra(name.clone()), line: Line { value: value.clone(), tail: String::new() } }));
    // A foreign word stays untouched and shows here, so a tag another app set is visible.
    if !facts.tags.is_empty() {
        out.push(pair(Label::Tags, facts.tags.join(", ")));
    }
    out.push(pair(Label::Modified, i18n.locale().format_date(facts.modified).unwrap_or_default()));
    out.push(pair(Label::Permissions, facts.mode.clone()));
    let owner = if facts.owner.is_empty() { i18n.tr("dialogs.file.properties.unknown") } else { facts.owner.clone() };
    out.push(pair(Label::Owner, i18n.tr_args("dialogs.file.properties.owner_value", &[&owner, &facts.uid.to_string()])));
    out.push(pair(Label::Group, i18n.tr_args("dialogs.file.properties.group_value", &[&facts.gid.to_string()])));
    if facts.symlink {
        out.push(pair(Label::LinkTarget, facts.target.clone()));
    }
    out
}

/// The several-item reply: kinds, combined bytes and the selected entries' own modified extrema.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Several {
    pub kinds: String,
    pub bytes: u64,
    pub newest: String,
    pub oldest: String,
    pub partial: bool,
    pub done: bool,
    pub cancelled: bool,
}

impl Several {
    pub fn from_reply(reply: &Json) -> Self {
        Self {
            kinds: text(reply, "kinds"),
            bytes: number(reply, "bytes").max(0.) as u64,
            newest: text(reply, "newest"),
            oldest: text(reply, "oldest"),
            partial: flag(reply, "partial"),
            done: flag(reply, "done"),
            cancelled: flag(reply, "cancelled"),
        }
    }
}

/// Kinds, Combined, Newest and Oldest only; incomplete Combined is unknown at zero, otherwise a floor.
pub fn several_pairs(i18n: &I18n, several: &Several) -> Vec<Pair> {
    let exact = several.done && !several.partial;
    let size = i18n.locale().format_size(several.bytes);
    let combined = Line {
        value: if !exact && several.bytes == 0 {
            i18n.tr("dialogs.file.properties.unknown")
        } else if exact {
            size
        } else {
            i18n.tr_args("dialogs.file.properties.combined_floor", &[&size])
        },
        tail: tail(i18n, !several.done),
    };
    vec![
        pair(Label::Kinds, several.kinds.clone()),
        Pair { name: Name::Own(Label::Combined), line: combined },
        pair(Label::Newest, several.newest.clone()),
        pair(Label::Oldest, several.oldest.clone()),
    ]
}

/// A refusal the backend names by its fixed sentence, mapped to the card's own words; any other text stays the backend's.
pub fn refusal(i18n: &I18n, error: &str) -> String {
    const KNOWN: [(&str, &str); 9] = [
        ("The selected items stopped answering.", "dialogs.file.properties.err_stopped"),
        ("Selected items changed; reopen the menu.", "dialogs.file.properties.err_items_changed"),
        ("Selected item moved or disappeared; reopen the menu.", "dialogs.file.properties.err_moved"),
        ("Selected item changed; reopen the menu.", "dialogs.file.properties.err_item_changed"),
        ("Could not format the selected items' modification dates.", "dialogs.file.properties.err_dates"),
        ("The selected items are too large to total.", "dialogs.file.properties.err_total"),
        ("A menu request is still running; try again when it finishes.", "dialogs.file.properties.err_running"),
        ("Menu request cancelled.", "dialogs.file.properties.err_cancelled"),
        ("The menu stopped answering; reopen this window.", "dialogs.file.properties.err_menu_stopped"),
    ];
    match KNOWN.iter().find(|(sentence, _)| *sentence == error) {
        Some((_, key)) => i18n.tr(key),
        None if error.is_empty() => i18n.tr("dialogs.file.properties.err_generic"),
        None => error.to_string(),
    }
}

pub fn properties_request(id: u64) -> String {
    format!(r#"{{"c":"menuaction","op":"properties","id":{id}}}"#)
}

pub fn contents_request(id: u64) -> String {
    format!(r#"{{"c":"menuaction","op":"contents","id":{id}}}"#)
}

pub fn facts_request(id: u64) -> String {
    format!(r#"{{"c":"menuaction","op":"facts","id":{id}}}"#)
}

/// The snapshot that captures the selection before Properties asks; it names the listing its rows were read from, as the menu's does.
pub fn snapshot_request(id: u64, listing: usize, rows: &[usize], cursor: usize) -> String {
    let rows: Vec<String> = rows.iter().map(usize::to_string).collect();
    format!(r#"{{"c":"menuaction","op":"snapshot","id":{id},"listing":{listing},"rows":[{}],"cursor":{cursor}}}"#, rows.join(","))
}
