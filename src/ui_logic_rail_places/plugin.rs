//! Plugin rail sections: a `section` answer read as untrusted rows, drawn after every built-in section.
use super::order::{Heading, RailSection};
use super::rail::{Group, Kind, RailEntry, Square};
use super::{has_control, js_trim};
use crate::jsondoc::{self, Json};

// refreshSec is clamped to this range, as the plugin reference states.
pub const REFRESH_MIN: u32 = 5;
pub const REFRESH_MAX: u32 = 3600;
// One section draws at most this many rows; a longer answer keeps its first rows.
pub const ROWS_MAX: usize = 64;
// The rail marks a plugin row may name; anything else draws the folder mark.
pub const GLYPHS: [&str; 7] = [
    "folder", "server", "drive", "network", "globe", "cloud", "phone",
];
// A manifest that names no refreshSec asks at the plugin reference's own example period.
pub const REFRESH_DEFAULT: u32 = 30;
// An id or label longer than this many characters, or an opens longer than this many bytes, is untrusted bulk.
pub const TEXT_MAX: usize = 256;
pub const OPENS_MAX: usize = 4096;

// What the plugin service (st2) publishes about one plugin with a rail-section kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub plugin: String,
    // The section title from the plugin's own catalog, already resolved to the current language.
    pub title: String,
    pub refresh_sec: u32,
    pub enabled: bool,
    pub failed: bool,
}

pub fn clamp_refresh(sec: Option<f64>) -> u32 {
    sec.filter(|s| s.is_finite()).map_or(REFRESH_DEFAULT, |s| {
        s.round().clamp(REFRESH_MIN as f64, REFRESH_MAX as f64) as u32
    })
}

// A C0, C1 or bidirectional control, which a plugin's text may never carry into the rail.
fn unsafe_text(text: &str) -> bool {
    has_control(text)
        || text.chars().any(|c| matches!(c, '\u{80}'..='\u{9f}' | '\u{61c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
}

// A path is clean when neither it nor its decoded form has a dot segment, and the decoded form carries no control.
fn clean_path(path: &str) -> bool {
    let dotted = |path: &str| path.split('/').any(|seg| seg == "." || seg == "..");
    let decoded = super::decode_path(path);
    !dotted(path) && !dotted(&decoded) && !unsafe_text(&decoded)
}

fn opens_ok(opens: &str) -> bool {
    if opens.len() > OPENS_MAX {
        return false;
    }
    if opens.starts_with('/') {
        return clean_path(opens);
    }
    if let Some(path) = opens.strip_prefix("file://") {
        return path.starts_with('/') && clean_path(path);
    }
    let Some((scheme, rest)) = opens.split_once("://") else {
        return false;
    };
    !rest.is_empty()
        && clean_path(&rest[rest.find('/').unwrap_or(rest.len())..])
        && scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
}

// Sample input: {"op":"section","rows":[{"id":"nas","label":"nas","opens":"sftp://nas/"}]}; a bad answer is None (last rows stay), a bad row drops alone.
pub fn parse_section(plugin: &str, line: &str) -> Option<Vec<RailEntry>> {
    let doc = jsondoc::parse(line).ok()?;
    if doc.get("op").and_then(Json::as_str) != Some("section") {
        return None;
    }
    let rows = doc.get("rows").and_then(Json::as_array)?;
    let mut out: Vec<RailEntry> = Vec::new();
    for row in rows {
        if out.len() >= ROWS_MAX {
            break;
        }
        let text = |key: &str| {
            row.get(key)
                .and_then(Json::as_str)
                .filter(|v| !unsafe_text(v))
        };
        let (Some(id), Some(label), Some(opens)) = (text("id"), text("label"), text("opens"))
        else {
            continue;
        };
        let label = js_trim(label);
        if id.is_empty()
            || id.chars().count() > TEXT_MAX
            || label.is_empty()
            || !opens_ok(opens)
            || out.iter().any(|seen| seen.id == id)
        {
            continue;
        }
        let label: String = label.chars().take(TEXT_MAX).collect();
        let square = match row.get("state").and_then(Json::as_str) {
            Some("on") => Square::On,
            Some("off") => Square::Off,
            _ => Square::None,
        };
        let glyph = text("glyph")
            .and_then(|g| GLYPHS.iter().find(|known| **known == g))
            .copied()
            .unwrap_or("folder");
        // A local opens, plain or file://, is a decoded path the rail can light; anything else is a remote uri.
        let local = opens.strip_prefix("file://").map_or_else(
            || opens.starts_with('/').then(|| opens.to_owned()),
            |p| Some(super::decode_path(p)),
        );
        out.push(RailEntry {
            group: Group::Plugin,
            kind: Kind::PluginRow,
            label,
            uri: if local.is_some() {
                String::new()
            } else {
                opens.to_owned()
            },
            path: local.unwrap_or_default(),
            glyph,
            square,
            section: plugin.to_owned(),
            id: id.to_owned(),
            ..RailEntry::default()
        });
    }
    Some(out)
}

// One section: its title as the plugin catalog resolves it, set in capitals where the rail draws it, and its rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub plugin: String,
    pub heading: String,
    pub rows: Vec<RailEntry>,
}

// The sections in descriptor order: only enabled plugins that have not failed and answered rows; an empty one draws no heading.
pub fn sections(descriptors: &[Descriptor], answers: &[(String, Vec<RailEntry>)]) -> Vec<Section> {
    descriptors
        .iter()
        .filter(|d| d.enabled && !d.failed)
        .filter_map(|d| {
            let rows = answers
                .iter()
                .find(|(plugin, _)| *plugin == d.plugin)
                .map(|(_, rows)| rows)?;
            (!rows.is_empty()).then(|| Section {
                plugin: d.plugin.clone(),
                heading: d.title.clone(),
                rows: rows.clone(),
            })
        })
        .collect()
}

// The heading Flea draws over plugin rows that name no title of their own: the Settings page's own word.
pub const PLUGINS_HEADING: &str = "settings.section.plugins";
// Every heading the rail draws for itself; a plugin title that reads as one of these goes under PLUGINS.
pub const BUILT_IN_HEADINGS: [&str; 7] = [
    "places.heading.places",
    "places.heading.favorites",
    "places.heading.tags",
    "places.heading.network",
    "settings.places.network",
    "settings.places.devices",
    PLUGINS_HEADING,
];

// Sample: SYNCTHING from two plugins, then MEGA and a title "Devices" under one PLUGINS. Built-in sections keep their
// places; plugin sections merge by title as `drawn` sets it, in plugin order, and untitled or clashing rows go last.
pub fn plugin_headings(
    sections: Vec<RailSection>,
    drawn: impl Fn(&Heading) -> String,
) -> Vec<RailSection> {
    let reserved: Vec<String> = BUILT_IN_HEADINGS
        .iter()
        .map(|key| drawn(&Heading::Key(key)).trim().to_owned())
        .collect();
    let mut out = Vec::new();
    let mut titled: Vec<(String, RailSection)> = Vec::new();
    let mut untitled = Vec::new();
    for section in sections {
        let Heading::Plugin(_) = &section.heading else {
            out.push(section);
            continue;
        };
        let set = drawn(&section.heading).trim().to_owned();
        if set.is_empty() || reserved.contains(&set) {
            untitled.extend(section.rows);
        } else if let Some((_, group)) = titled.iter_mut().find(|(seen, _)| *seen == set) {
            group.rows.extend(section.rows);
        } else {
            titled.push((set, section));
        }
    }
    out.extend(titled.into_iter().map(|(_, section)| section));
    out.push(RailSection {
        heading: Heading::Key(PLUGINS_HEADING),
        rows: untitled,
    });
    out.retain(|s| !s.rows.is_empty());
    out
}

// Built-in sections first, then plugin rows, so PLACES, NETWORK and DEVICES never move when a plugin comes or goes.
pub fn with_plugins(built_in: &[RailEntry], sections: &[Section]) -> Vec<RailEntry> {
    built_in
        .iter()
        .cloned()
        .chain(sections.iter().flat_map(|s| s.rows.iter().cloned()))
        .collect()
}
