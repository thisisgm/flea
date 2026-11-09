//! The flash card's rendering: one render of every line a flash draws, stored on the transfer by the runner.
use crate::i18n::I18n;

// Sample input: 73298 ms reads "1:13"; 57692 ms reads "0:57".
pub fn duration(ms: u64) -> String {
    let total = ms / 1000;
    let hours = total / 3600;
    let minutes = total % 3600 / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{:02}:{seconds:02}", minutes)
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// The running flash the runner draws, in the backend's own fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transfer {
    pub phase: String,
    pub drive: String,
    pub name: String,
    pub bytes: u64,
    pub total: u64,
    pub known: bool,
    pub permille: u64,
}

/// One drawn byte-line span: figures in foreground, the rest muted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub text: String,
    pub figure: bool,
}

// Sample input: "Flashing 128GB" while writing; "Verifying 128GB" while verifying.
pub fn head(i18n: &I18n, transfer: &Transfer) -> String {
    let key =
        if transfer.phase == "verify" { "archive.flash.head_verifying" } else { "archive.flash.head_writing" };
    i18n.tr_args(key, &[&transfer.drive])
}

// Sample input: a known total reads "debian-13.iso · 4.0 GB"; an unknown one the name alone.
pub fn file(i18n: &I18n, transfer: &Transfer) -> String {
    if transfer.total > 0 {
        let size = i18n.locale().format_size(transfer.total);
        i18n.tr_args("archive.flash.file_known", &[&transfer.name, &size])
    } else {
        transfer.name.clone()
    }
}

// The total a flash measures against: the known size, else the unpacker's share scaled up, else 0.
pub fn total(transfer: &Transfer) -> u64 {
    if transfer.known && transfer.total > 0 {
        return transfer.total;
    }
    if transfer.permille > 0 && transfer.bytes > 0 {
        return transfer.bytes.saturating_mul(1000) / transfer.permille;
    }
    0
}

// Sample input: 1.2 of 4.0 GB at 38.2 MB/s leaves "1.2 GB of 4.0 GB · 38.2 MB/s · 1:13 left".
pub fn parts(i18n: &I18n, transfer: &Transfer, rate: u64) -> Vec<Part> {
    let locale = i18n.locale();
    if transfer.phase == "prepare" || transfer.phase == "password" {
        return vec![Part { text: i18n.tr("archive.flash.wait_password"), figure: false }];
    }
    let figure = |text: String| Part { text, figure: true };
    if transfer.phase == "flush" {
        if transfer.bytes == 0 {
            return Vec::new();
        }
        return lined(&i18n.tr("archive.flash.flush"), &[figure(locale.format_size(transfer.bytes))]);
    }
    if transfer.bytes == 0 {
        return Vec::new();
    }
    let whole = total(transfer);
    let moved = figure(locale.format_size(transfer.bytes));
    let speed = figure(locale.format_size(rate));
    let unit = figure("/s".into());
    let left = if whole > 0 && rate > 0 {
        Some(figure(duration(whole.saturating_sub(transfer.bytes).saturating_mul(1000) / rate)))
    } else {
        None
    };
    if transfer.known && transfer.total > 0 {
        let total = figure(locale.format_size(whole));
        if let Some(left) = left {
            return lined(&i18n.tr("archive.flash.known"), &[moved, total, speed, unit, left]);
        }
        return lined(&i18n.tr("archive.flash.known_stalled"), &[moved, total, speed, unit]);
    }
    if let Some(left) = left {
        return lined(&i18n.tr("archive.flash.unknown"), &[moved, speed, unit, left]);
    }
    lined(&i18n.tr("archive.flash.unknown_stalled"), &[moved, speed, unit])
}

// Sample input: "%1 of %2" with figures "1.2 GB" and "4.0 GB" reads figure, " of ", figure.
pub(super) fn lined(template: &str, args: &[Part]) -> Vec<Part> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut rest = template;
    while let Some(at) = rest.find('%') {
        text.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = 1 + rest.as_bytes()[1..].iter().take_while(|b| b.is_ascii_digit()).count();
        let index = rest[1..end].parse::<usize>().ok().filter(|n| *n > 0);
        match index.and_then(|n| args.get(n - 1)) {
            Some(part) => {
                if !text.is_empty() {
                    out.push(Part { text: std::mem::take(&mut text), figure: false });
                }
                out.push(part.clone());
            }
            None => text.push_str(&rest[..end]),
        }
        rest = &rest[end..];
    }
    text.push_str(rest);
    if !text.is_empty() {
        out.push(Part { text, figure: false });
    }
    out
}

// Sample input: a known total fills 0.3; an unknown one fills against the permille.
pub fn share(transfer: &Transfer) -> f64 {
    let at = if transfer.known && transfer.total > 0 {
        transfer.bytes as f64 / transfer.total as f64
    } else {
        transfer.permille as f64 / 1000.0
    };
    at.clamp(0.0, 1.0)
}

/// Every line a flash draws beside its fields.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn {
    pub head_text: String,
    pub file_text: String,
    pub byte_text: Vec<Part>,
    pub share: f64,
    pub no_cancel: bool,
}

// Sample input: Cancel runs while writing and dims in the final flush, when every byte is on the device.
pub fn render(i18n: &I18n, transfer: &Transfer, rate: u64) -> Drawn {
    Drawn {
        head_text: head(i18n, transfer),
        file_text: file(i18n, transfer),
        byte_text: parts(i18n, transfer, rate),
        share: share(transfer),
        no_cancel: transfer.phase == "flush",
    }
}
