//! A send's words in the transfer card's grammar and the status bar's receipt, every one through the catalog.
//! Words are chosen from the job's structured fields and the front end's own request, never from the backend's English heads.
use super::send::{Phase, Send};
use super::FolderRefusal;
use crate::i18n::{I18n, Locale};

/// One run of the byte line: a figure inked in the foreground, or the muted words between figures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub text: String,
    pub figure: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bar {
    /// The failed block draws no bar.
    None,
    /// Filled share of the track, 0 to 1, the whole send rather than the one file.
    Fill(f32),
    /// The check's accent segment sweeping the track; see `super::sweep`.
    Sweep,
}

/// One send's block in the transfer card, laid out as v0.3.12's one-job anatomy.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub head: String,
    pub file: String,
    pub bar: Bar,
    pub bytes: Vec<Part>,
    /// Cancel's label and whether it still takes a press.
    pub cancel: Option<(String, bool)>,
    pub retry: Option<String>,
    pub dismiss: Option<String>,
    /// Draws the head in the error role with the alert mark.
    pub failed: bool,
}

/// The status bar's receipt: the sentence in the foreground, no full stop, and the check muted after it with its separator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub text: String,
    pub hint: String,
}

pub fn row_label(i18n: &I18n) -> String {
    i18n.tr("send.rclone.row")
}

/// Sample: 3 items to gdrive is "Send 3 Items to gdrive"; the remote's name keeps its own case.
pub fn title(i18n: &I18n, items: usize, remote: &str) -> String {
    i18n.trn_args("send.rclone.title", items as i64, &[remote])
}

pub fn folder_label(i18n: &I18n) -> String {
    i18n.tr("send.rclone.folder")
}

pub fn send_label(i18n: &I18n) -> String {
    i18n.tr("send.rclone.send")
}

pub fn cancel_label(i18n: &I18n) -> String {
    i18n.tr("send.rclone.cancel")
}

/// The line under the card's field: a refused folder says why, a refused send carries the backend's words as a diagnostic.
pub fn card_error(
    i18n: &I18n,
    refusal: Option<FolderRefusal>,
    send: Option<&Send>,
) -> Option<String> {
    if refusal == Some(FolderRefusal::Invalid) {
        return Some(i18n.tr("send.rclone.folder_invalid"));
    }
    match send.map(|send| &send.phase) {
        Some(Phase::Refused(error)) => Some(i18n.tr_args("send.rclone.reason", &[error])),
        _ => None,
    }
}

/// The block a send shows while it runs or after it failed; any other ending leaves the card.
pub fn block(i18n: &I18n, send: &Send) -> Option<Block> {
    let locale = i18n.locale();
    let row = &send.row;
    let cancel = || {
        let key = if send.cancelling {
            "send.rclone.cancelling"
        } else {
            "send.rclone.cancel"
        };
        Some((i18n.tr(key), !send.cancelling && send.job.is_some()))
    };
    let failed = |reason: &str| Block {
        head: i18n.tr("send.rclone.failed"),
        file: if reason.is_empty() {
            String::new()
        } else {
            i18n.tr_args("send.rclone.reason", &[reason])
        },
        bar: Bar::None,
        bytes: Vec::new(),
        cancel: None,
        // A retry in flight offers neither button until the backend answers it.
        retry: send
            .job
            .filter(|_| send.phase != Phase::Lost && !send.retrying)
            .map(|_| i18n.tr("send.rclone.retry")),
        dismiss: (!send.retrying).then(|| i18n.tr("send.rclone.dismiss")),
        failed: true,
    };
    Some(match &send.phase {
        Phase::Asked | Phase::Sending => Block {
            head: i18n.tr_args(
                "send.rclone.sending",
                &[
                    &locale.format_count((row.done + 1).min(row.total.max(1)) as i64),
                    &locale.format_count(row.total as i64),
                    &send.remote,
                ],
            ),
            file: file_line(i18n, send),
            bar: Bar::Fill(fraction(send)),
            bytes: byte_parts(i18n, send),
            cancel: cancel(),
            retry: None,
            dismiss: None,
            failed: false,
        },
        Phase::Checking(checksums) => Block {
            head: i18n.trn_args("send.rclone.checking", row.total as i64, &[&send.remote]),
            file: i18n.tr(if *checksums {
                "send.rclone.comparing_checksums"
            } else {
                "send.rclone.comparing_sizes"
            }),
            bar: Bar::Sweep,
            bytes: Vec::new(),
            cancel: cancel(),
            retry: None,
            dismiss: None,
            failed: false,
        },
        Phase::Failed(reason) => failed(reason),
        Phase::Lost => failed(""),
        Phase::Refused(_) | Phase::Receipt { .. } | Phase::Gone => return None,
    })
}

/// The receipt a finished send leaves in the status bar, counting what this attempt sent.
pub fn receipt(i18n: &I18n, send: &Send) -> Option<Receipt> {
    let Phase::Receipt { check, note } = &send.phase else {
        return None;
    };
    let count = send.row.done as i64;
    let text = if send.folder.is_empty() {
        i18n.trn_args("send.rclone.sent_root", count, &[&send.remote])
    } else {
        i18n.trn_args("send.rclone.sent", count, &[&send.folder, &send.remote])
    };
    // The wire's check and note are protocol words, never prose: checksum, size, or the not checked note.
    let hint = match (check.as_str(), note.as_str()) {
        ("checksum", _) => "send.rclone.checked_checksum",
        ("size", _) => "send.rclone.checked_size",
        (_, "not checked") => "send.rclone.not_checked",
        _ => "",
    };
    // The muted run starts with the card's separator, as rclone-sent draws " · checked by checksum".
    let hint = if hint.is_empty() {
        String::new()
    } else {
        i18n.tr("send.rclone.sep") + &i18n.tr(hint)
    };
    Some(Receipt { text, hint })
}

fn file_line(i18n: &I18n, send: &Send) -> String {
    let row = &send.row;
    if row.file.is_empty() || row.item_bytes == 0 {
        return row.file.clone();
    }
    i18n.tr_args(
        "send.rclone.file",
        &[&row.file, &i18n.locale().format_size(row.item_bytes)],
    )
}

/// The share of the whole send that has landed; with no byte total yet it counts finished items.
pub fn fraction(send: &Send) -> f32 {
    let row = &send.row;
    let share = if row.total_bytes > 0 {
        row.bytes as f32 / row.total_bytes as f32
    } else if row.total > 0 {
        row.done as f32 / row.total as f32
    } else {
        0.
    };
    share.clamp(0., 1.)
}

/// Sample: 795.1 kB of 1.7 MB · 240.0 kB/s · 0:03 left; the estimate is rclone's own, or worked from the rate.
pub fn byte_parts(i18n: &I18n, send: &Send) -> Vec<Part> {
    let (locale, row) = (i18n.locale(), &send.row);
    if row.bytes == 0 {
        return Vec::new();
    }
    let figure = |text: String| Part { text, figure: true };
    let word = |key: &str| Part {
        text: i18n.tr(key),
        figure: false,
    };
    let mut parts = vec![figure(locale.format_size(row.bytes))];
    if row.total_bytes > 0 {
        parts.extend([
            word("send.rclone.of"),
            figure(locale.format_size(row.total_bytes)),
        ]);
    } else {
        parts.push(word("send.rclone.sent_bytes"));
    }
    parts.extend([
        word("send.rclone.sep"),
        figure(i18n.tr_args("send.rclone.rate", &[&locale.format_size(row.rate)])),
    ]);
    let left = row.left.or_else(|| {
        (row.total_bytes > 0 && row.rate > 0)
            .then(|| row.total_bytes.saturating_sub(row.bytes) / row.rate)
    });
    if let Some(seconds) = left.filter(|_| row.total_bytes > 0) {
        parts.extend([
            word("send.rclone.sep"),
            figure(duration(locale, seconds)),
            word("send.rclone.left"),
        ]);
    }
    parts
}

/// Sample: 3 is 0:03 and 3725 is 1:02:05, in the language's own digits.
pub fn duration(locale: &Locale, seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    let text = if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    };
    text.split(':')
        .map(|part| locale.localize_number(part))
        .collect::<Vec<_>>()
        .join(":")
}
