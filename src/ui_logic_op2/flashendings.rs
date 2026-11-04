//! Every ending is a row in the Flash step with its fix in a hint; only the mark reads error.
use crate::i18n::I18n;

/// One ending: the headline, the hint with its fix, which button comes back, and the status line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ending {
    pub headline: String,
    pub hint: String,
    pub role: String,
    pub mark: String,
    pub eject: bool,
    pub flash_back: bool,
    /// The status line as one whole sentence, never the headline with punctuation appended.
    pub line: String,
}

/// The figures an ending names: the image, the drive, where the drive stood and against what total.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ctx {
    pub image: String,
    pub drive: String,
    pub at: u64,
    pub total: u64,
    pub blank: bool,
    pub tool: String,
    pub phase: String,
    pub drive_bytes: u64,
}

// Sample input: done names the image and the drive and offers Eject; a dismissed prompt wrote nothing.
pub fn ending_for(i18n: &I18n, err: &str, ctx: &Ctx) -> Ending {
    let locale = i18n.locale();
    let at = locale.format_size(ctx.at);
    let total = locale.format_size(ctx.total);
    let (headline, hint, role, mark, eject, flash_back, line) = match err {
        "" => (
            i18n.tr_args("archive.flash.done", &[&ctx.image, &ctx.drive]),
            String::new(),
            "foreground",
            "check",
            true,
            false,
            i18n.tr_args("archive.flash.done_line", &[&ctx.image, &ctx.drive]),
        ),
        "cancelled" if ctx.phase == "verify" => (
            i18n.tr("archive.flash.verify_cancelled"),
            i18n.tr_args("archive.flash.verify_cancelled_hint", &[&ctx.image, &ctx.drive]),
            "foreground",
            "x",
            true,
            false,
            i18n.tr("archive.flash.verify_cancelled_line"),
        ),
        "cancelled" if ctx.phase == "prepare" || ctx.phase == "password" => (
            i18n.tr("archive.flash.prewrite"),
            i18n.tr("archive.flash.prewrite_hint"),
            "foreground",
            "x",
            false,
            true,
            i18n.tr("archive.flash.prewrite_line"),
        ),
        "cancelled" if ctx.total == 0 => (
            i18n.tr_args("archive.flash.cancelled_at_unknown", &[&at]),
            i18n.tr_args("archive.flash.cancelled_hint", &[&ctx.drive]),
            "foreground",
            "x",
            false,
            true,
            i18n.tr_args("archive.flash.cancelled_at_unknown_line", &[&at]),
        ),
        "cancelled" => (
            i18n.tr_args("archive.flash.cancelled_at", &[&at, &total]),
            i18n.tr_args("archive.flash.cancelled_hint", &[&ctx.drive]),
            "foreground",
            "x",
            false,
            true,
            i18n.tr_args("archive.flash.cancelled_at_line", &[&at, &total]),
        ),
        "denied" => (
            i18n.tr("archive.flash.prewrite"),
            i18n.tr("archive.flash.denied_hint"),
            "error",
            "alert",
            false,
            true,
            i18n.tr("archive.flash.prewrite_line"),
        ),
        "busy" => (
            i18n.tr_args("archive.flash.unmount", &[&ctx.drive]),
            i18n.tr("archive.flash.busy_hint"),
            "error",
            "alert",
            false,
            true,
            i18n.tr_args("archive.flash.unmount_line", &[&ctx.drive]),
        ),
        "changed" => (
            i18n.tr_args("archive.flash.changed", &[&ctx.drive]),
            i18n.tr("archive.flash.choose_again"),
            "error",
            "alert",
            false,
            true,
            i18n.tr_args("archive.flash.changed_line", &[&ctx.drive]),
        ),
        "gone" => (
            i18n.tr_args("archive.flash.unplugged", &[&ctx.drive]),
            i18n.tr_args("archive.flash.left_at", &[&at]),
            "error",
            "alert",
            false,
            true,
            i18n.tr_args("archive.flash.unplugged_line", &[&ctx.drive]),
        ),
        "io" => {
            let hint = if ctx.blank {
                i18n.tr_args("archive.flash.blank_hint", &[&at])
            } else {
                i18n.tr_args("archive.flash.partial_hint", &[&at])
            };
            (
                i18n.tr_args("archive.flash.io", &[&ctx.drive]),
                hint,
                "error",
                "alert",
                false,
                true,
                i18n.tr_args("archive.flash.io_line", &[&ctx.drive]),
            )
        }
        "nospace" => {
            let past = locale.format_size(ctx.drive_bytes);
            (
                i18n.tr_args("archive.flash.too_big", &[&ctx.image, &ctx.drive]),
                i18n.tr_args("archive.flash.unpacked_past", &[&past]),
                "error",
                "alert",
                false,
                true,
                i18n.tr_args("archive.flash.too_big_line", &[&ctx.image, &ctx.drive]),
            )
        }
        "verify" => (
            i18n.tr_args("archive.flash.not_verify", &[&ctx.drive]),
            i18n.tr_args("archive.flash.verify_hint", &[&at]),
            "error",
            "alert",
            false,
            true,
            i18n.tr_args("archive.flash.verify_again", &[&ctx.drive]),
        ),
        "decompress" => (
            i18n.tr_args("archive.flash.no_unpack", &[&ctx.image]),
            ctx.tool.clone(),
            "error",
            "alert",
            false,
            true,
            i18n.tr_args("archive.flash.no_unpack_line", &[&ctx.image]),
        ),
        "sandbox" => (
            i18n.tr("archive.flash.need_bubble"),
            i18n.tr("archive.flash.install_bubble"),
            "error",
            "alert",
            false,
            true,
            i18n.tr("archive.flash.need_bubble_line"),
        ),
        _ => (
            i18n.tr_args("archive.flash.no_open", &[&ctx.drive]),
            ctx.tool.clone(),
            "error",
            "alert",
            false,
            true,
            i18n.tr_args("archive.flash.no_open_line", &[&ctx.drive]),
        ),
    };
    Ending {
        headline,
        hint,
        role: role.into(),
        mark: mark.into(),
        eject,
        flash_back,
        line,
    }
}

/// What a flash that ends while the listing shows reports in the status line, as a copy does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub is_error: bool,
}

// Sample input: done reports "Flashed debian-13.iso to 128GB."; a verify miss reports as an error.
pub fn status_for(i18n: &I18n, err: &str, ctx: &Ctx) -> Status {
    if err.is_empty() {
        return Status {
            text: i18n.tr_args("archive.flash.done_line", &[&ctx.image, &ctx.drive]),
            is_error: false,
        };
    }
    if err == "cancelled" {
        let ending = ending_for(i18n, err, ctx);
        return Status { text: ending.line, is_error: false };
    }
    if err == "verify" {
        return Status {
            text: i18n.tr_args("archive.flash.verify_again", &[&ctx.drive]),
            is_error: true,
        };
    }
    let ending = ending_for(i18n, err, ctx);
    Status { text: ending.line, is_error: true }
}

// Sample input: "Ejected 128GB. It is safe to unplug."
pub fn eject_done_line(i18n: &I18n, drive: &str) -> String {
    i18n.tr_args("archive.flash.ejected", &[drive])
}

// Sample input: "128GB is still mounted; close what is using it."
pub fn eject_busy_line(i18n: &I18n, drive: &str) -> String {
    i18n.tr_args("archive.flash.still_mounted", &[drive])
}

// Sample input: "Could not eject 128GB."
pub fn eject_failed_line(i18n: &I18n, drive: &str) -> String {
    i18n.tr_args("archive.flash.eject_failed", &[drive])
}
