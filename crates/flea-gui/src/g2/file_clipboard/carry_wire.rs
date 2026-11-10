//! Captured carries on the existing window socket, with exact integer and filename text.
use super::carry::Flock;
use crate::row::marks::{Language, Mark};
use flea::jsondoc::{self, Json};
use flea::ui_logic_file_clipboard::carry::{Lifted, Source};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const LIFETIME_MS: u128 = 60_000;
// A window has a left and a right pane.
const PANE_SIDES: usize = 2;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Offer {
    pub pid: u32,
    pub stamp: u128,
    pub lift_stamp: u128,
    pub expires: u128,
    pub token: String,
    pub lifted: Lifted,
    pub flock: Flock,
}

// Fixture processes move wall time with the executor clock, so leases age in tests.
#[cfg(test)]
pub(super) static TEST_CLOCK_SKEW_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[cfg(test)]
fn skew_ms() -> u128 {
    TEST_CLOCK_SKEW_MS.load(std::sync::atomic::Ordering::Relaxed) as u128
}

#[cfg(not(test))]
const fn skew_ms() -> u128 {
    0
}

pub(super) fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        + skew_ms()
}

fn text(value: impl ToString) -> Json {
    Json::Str(value.to_string())
}

fn object(fields: &[(&str, Json)]) -> Json {
    Json::Obj(
        fields
            .iter()
            .map(|(key, value)| ((*key).into(), value.clone()))
            .collect(),
    )
}

fn string<'a>(value: &'a Json, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

fn number<T: std::str::FromStr>(value: &Json, key: &str) -> Option<T> {
    string(value, key)?.parse().ok()
}

fn strings(value: &Json, key: &str) -> Option<Vec<String>> {
    value
        .get(key)?
        .as_array()?
        .iter()
        .map(|item| Some(item.as_str()?.to_owned()))
        .collect()
}

fn mark_name(mark: Mark) -> String {
    match mark {
        Mark::Language(language) => format!("language:{}", language.slug()),
        _ => mark.name().into(),
    }
}

fn mark(value: &str) -> Option<Mark> {
    if let Some(slug) = value.strip_prefix("language:") {
        return Language::for_slug(slug).map(Mark::Language);
    }
    Some(match value {
        "folder" => Mark::Folder,
        "folder-open" => Mark::FolderOpen,
        "file" => Mark::File,
        "file-text" => Mark::FileText,
        "code" => Mark::Code,
        "image" => Mark::Image,
        "film" => Mark::Film,
        "music" => Mark::Music,
        "archive" => Mark::Archive,
        "type" => Mark::Type,
        "terminal" => Mark::Terminal,
        "table" => Mark::Table,
        "presentation" => Mark::Presentation,
        "symlink" => Mark::Symlink,
        _ => return None,
    })
}

impl Offer {
    pub fn new(lifted: Lifted, flock: Flock, previous: u128) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .max(previous + 1);
        let pid = std::process::id();
        Self {
            pid,
            stamp,
            lift_stamp: stamp,
            expires: now() + LIFETIME_MS,
            token: format!("{pid}-{stamp}"),
            lifted,
            flock,
        }
    }

    pub fn live(&self) -> bool {
        let remaining = self.expires.saturating_sub(now());
        remaining > 0 && remaining <= LIFETIME_MS
    }

    pub fn encode(&self) -> String {
        let lifted = &self.lifted;
        jsondoc::render(&object(&[
            ("pid", text(self.pid)),
            ("stamp", text(self.stamp)),
            ("liftStamp", text(self.lift_stamp)),
            ("expires", text(self.expires)),
            ("token", text(&self.token)),
            ("window", text(lifted.source.window)),
            ("side", text(lifted.source.side)),
            ("folder", text(&lifted.source.folder)),
            ("listing", text(lifted.source.listing)),
            ("serial", text(lifted.serial)),
            ("rows", Json::Arr(lifted.rows.iter().map(text).collect())),
            ("paths", Json::Arr(lifted.paths.iter().map(text).collect())),
            (
                "marks",
                Json::Arr(
                    self.flock
                        .marks
                        .iter()
                        .map(|mark| text(mark_name(*mark)))
                        .collect(),
                ),
            ),
            ("name", text(&self.flock.name)),
        ]))
    }

    // Sample input: {"pid":"4242","stamp":"17","liftStamp":"17","expires":"60017","token":"4242-17","window":"1","side":"0","folder":"/h","listing":"3","serial":"1","rows":["2"],"paths":["/h/a"],"marks":["file"],"name":"a"}
    pub fn decode(value: &str) -> Option<Self> {
        let value = jsondoc::parse(value).ok()?;
        let pid = number::<u32>(&value, "pid").filter(|pid| *pid > 0 && *pid <= i32::MAX as u32)?;
        let stamp = number(&value, "stamp")?;
        let lift_stamp = number(&value, "liftStamp")?;
        if lift_stamp > stamp {
            return None;
        }
        let token = string(&value, "token")?.to_owned();
        if token != format!("{pid}-{stamp}") {
            return None;
        }
        let rows = strings(&value, "rows")?
            .iter()
            .map(|row| row.parse().ok())
            .collect::<Option<Vec<usize>>>()?;
        let paths = strings(&value, "paths")?;
        let folder = string(&value, "folder")?.to_owned();
        if paths.is_empty()
            || rows.len() != paths.len()
            || rows.windows(2).any(|pair| pair[0] >= pair[1])
            || !absolute(&folder)
            || paths.iter().any(|path| !absolute(path))
        {
            return None;
        }
        let marks = strings(&value, "marks")?
            .iter()
            .map(|name| mark(name))
            .collect::<Option<Vec<_>>>()?;
        if marks.len() > super::carry::LAYERS.len() {
            return None;
        }
        let offer = Self {
            pid,
            stamp,
            lift_stamp,
            token,
            expires: number(&value, "expires")?,
            lifted: Lifted {
                rows,
                paths,
                serial: number(&value, "serial")?,
                source: Source {
                    window: number(&value, "window")?,
                    side: number::<usize>(&value, "side").filter(|side| *side < PANE_SIDES)?,
                    folder,
                    listing: number(&value, "listing")?,
                },
            },
            flock: Flock {
                marks,
                name: string(&value, "name")?.into(),
            },
        };
        offer.live().then_some(offer)
    }
}

fn absolute(path: &str) -> bool {
    !path.contains('\0') && flea::bytename::text_path(path).is_absolute()
}
