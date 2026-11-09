//! The model behind the flash view; every string is a whole template filled by one function.
use crate::i18n::I18n;

/// One drive as flashtargets lists it, in /sys/block order; refused is "" when the drive takes the image.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Drive {
    pub dev: String,
    pub seq: u64,
    pub serial: String,
    pub label: String,
    pub model: String,
    pub bytes: u64,
    pub refused: String,
}

// Sample input: a drive refused for no reason fits; one refused "small" does not.
pub fn fits(drive: &Drive) -> bool {
    drive.refused.is_empty()
}

// An SD card reports through mmcblk, so its read-only row names the lock switch.
pub fn is_card(dev: &str) -> bool {
    dev.starts_with("mmcblk")
}

// Sample input: "small" reads too small; a card's "readonly" names its lock switch.
pub fn refused_hint_key(drive: &Drive) -> &'static str {
    match drive.refused.as_str() {
        "small" => "archive.flash.hint_small",
        "readonly" if is_card(&drive.dev) => "archive.flash.hint_lock",
        "readonly" => "archive.flash.hint_readonly",
        "holdsimage" => "archive.flash.hint_holds",
        "member" => "archive.flash.hint_member",
        _ => "",
    }
}

// Sample input: a repeated "128GB" gains its kernel name; a model-named drive keeps it alone.
pub fn drive_name(drive: &Drive, drives: &[Drive]) -> String {
    let label = if drive.label.is_empty() { drive.model.clone() } else { drive.label.clone() };
    let twins = drives
        .iter()
        .filter(|other| {
            let name = if other.label.is_empty() { &other.model } else { &other.label };
            *name == label
        })
        .count();
    if twins > 1 { format!("{label} ({})", drive.dev) } else { label }
}

// Sample input: the model beside its label; "" where the detail would repeat the label, twin suffix or not.
pub fn drive_detail(drive: &Drive) -> String {
    let label = if drive.label.is_empty() { &drive.model } else { &drive.label };
    if drive.model == *label { String::new() } else { drive.model.clone() }
}

// Sample input: one fitting drive is chosen for you; two choose none.
pub fn preselect(drives: &[Drive]) -> Option<usize> {
    let mut found = None;
    for (at, drive) in drives.iter().enumerate() {
        if !fits(drive) {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(at);
    }
    found
}

// Cursor positions: 0 is the image, 1 to n the drives, n + 1 the Flash step.
pub fn flash_pos(drive_count: usize) -> usize {
    1 + drive_count
}

// Sample input: down from the fitting drive skips the refused row for Flash.
// The image row is always a stop: with no image chosen, Enter there opens the chooser.
pub fn step(cursor: usize, delta: isize, drives: &[Drive], flash_live: bool) -> usize {
    let count = flash_pos(drives.len()) + 1;
    let mut at = cursor as isize + delta;
    while at >= 0 && (at as usize) < count {
        let atn = at as usize;
        if atn == 0 {
            return atn;
        }
        if atn > 0 && atn <= drives.len() && fits(&drives[atn - 1]) {
            return atn;
        }
        if atn == flash_pos(drives.len()) && flash_live {
            return atn;
        }
        at += delta;
    }
    cursor
}

// Sample input: an image and a fitting drive arm Flash; a refused drive greys it.
pub fn flash_live(image_ok: bool, drive: Option<&Drive>) -> bool {
    image_ok && drive.is_some_and(fits)
}

/// The greyed row's wording: the chosen drive named, or the drive in general when none is chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Words {
    pub erase: String,
    pub action: String,
}

// Sample input: 128GB chosen reads "Erases everything on 128GB" and "Flash 128GB".
pub fn flash_words(i18n: &I18n, image_ok: bool, drive: Option<&Drive>, drives: &[Drive]) -> Words {
    let name = drive.map(|d| drive_name(d, drives)).unwrap_or_default();
    if !image_ok || name.is_empty() {
        return Words {
            erase: i18n.tr("archive.flash.erase_bare"),
            action: i18n.tr("archive.flash.action_bare"),
        };
    }
    Words {
        erase: i18n.tr_args("archive.flash.erase_named", &[&name]),
        action: i18n.tr_args("archive.flash.action_named", &[&name]),
    }
}

// Sample input: one drive stays singular; 1204 group.
pub fn drives_line(i18n: &I18n, count: usize) -> String {
    i18n.trn("archive.flash.drives_line", count as i64)
}

// Sample input: opening on a second pane while a flash runs is refused with one sentence.
pub fn already_running(i18n: &I18n) -> String {
    i18n.tr("archive.flash.running")
}

// Sample input: the strip's Up names why it never leaves Flash.
pub fn up_unavailable(i18n: &I18n) -> String {
    i18n.tr("archive.flash.up_unavailable")
}

// Sample input: "sda1" names "sda"; "nvme0n1p2" names "nvme0n1"; "mmcblk0" names itself.
pub fn rail_disk(device: &str) -> String {
    let name = match device.rfind('/') {
        Some(cut) => &device[cut + 1..],
        None => device,
    };
    let stripped = name.trim_end_matches(|c: char| c.is_ascii_digit());
    if stripped.len() < name.len() {
        if stripped.ends_with('p') && stripped[..stripped.len() - 1].chars().any(|c| c.is_ascii_digit()) {
            return stripped[..stripped.len() - 1].to_string();
        }
        for stem in ["sd", "vd", "hd", "xvd"] {
            if let Some(rest) = stripped.strip_prefix(stem) {
                if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphabetic()) {
                    return format!("{stem}{rest}");
                }
            }
        }
    }
    name.to_string()
}

/// The card's image; refused marks a reply with no kind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Image {
    pub path: String,
    pub kind: String,
    pub bytes: u64,
    pub unpacked: u64,
    pub known: bool,
    pub refused: bool,
}

// Sample input: a raw image shows its size alone; a compressed one with a recorded size shows it unpacked.
pub fn image_detail(i18n: &I18n, image: &Image) -> String {
    let raw = image.kind == "iso" || image.kind == "img";
    if !raw && image.known && image.unpacked > 0 {
        let size = i18n.locale().format_size(image.unpacked);
        i18n.tr_args("archive.flash.unpacked", &[&size])
    } else {
        String::new()
    }
}

// Sample input: the card's own reply fills its image; the pane's zip check leaves a null.
pub fn image_from(current: Option<&Image>, path: &str, kind: &str, bytes: u64, unpacked: u64, known: bool) -> Option<Image> {
    let current = current?;
    if current.path != path {
        return None;
    }
    Some(Image {
        path: current.path.clone(),
        kind: kind.to_string(),
        bytes,
        unpacked,
        known,
        refused: kind.is_empty(),
    })
}

// Sample input: the erase line draws in the warning red; the running head in the foreground.
pub fn step_role(running: bool, ending_role: Option<&str>) -> &str {
    if let Some(role) = ending_role {
        return role;
    }
    if running { "foreground" } else { "error" }
}
