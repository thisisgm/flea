//! Selection rules (sameTap/doubleAction/activates port); marks stay PARTIAL.
use crate::i18n::I18n;

#[derive(Clone, Debug, PartialEq)]
pub enum DoubleAction {
    Open,
    Mark,
    Accept,
    MarkAccept,
    None,
}

pub fn same_tap(first_path: &str, row_path: &str) -> bool {
    !row_path.is_empty() && row_path == first_path
}

// Sample: same-tap answers MarkAccept; rebuilt lists never retarget.
pub fn double_action(mode: &str, multiple: bool, directory_req: bool, is_dir: bool, marked: bool, same: bool) -> DoubleAction {
    if is_dir {
        return DoubleAction::Open;
    }
    if mode == "save" || directory_req || mode == "savefiles" {
        return DoubleAction::None;
    }
    if !same {
        return DoubleAction::None;
    }
    if multiple {
        return DoubleAction::Mark;
    }
    if marked {
        DoubleAction::Accept
    } else {
        DoubleAction::MarkAccept
    }
}

// The picker never renames, so Return/Enter still activates.
pub fn activates(action: &str, key: i32, ret: i32, enter: i32) -> bool {
    if action == "open" || action == "pageForward" {
        return true;
    }
    if action != "rename" {
        return false;
    }
    key == ret || key == enter
}

// Skipped note: one plural key; "and N more" its own.
pub fn skipped_note(i18n: &I18n, selected: usize, total: usize, skipped: &[(&str, &str)]) -> String {
    let Some((first_path, first_why)) = skipped.first() else {
        return String::new();
    };
    let count = |n: usize| i18n.locale().format_count(n as i64);
    let more = skipped.len() - 1;
    let tail = if more == 0 {
        String::new()
    } else {
        i18n.trn("picker.browser.skipped_more", more as i64)
    };
    i18n.trn_args(
        "picker.browser.skipped",
        skipped.len() as i64,
        &[&count(selected), &count(total), first_path, first_why, &tail],
    )
}
