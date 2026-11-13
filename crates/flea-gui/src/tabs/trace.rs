//! FLEA_TRACE_TABDRAG=1 stage lines, worded as TabBar.qml and WindowBody.qml print them, which tests/ui.sh xwtab reads.
use std::sync::LazyLock;

fn on() -> bool {
    static ON: LazyLock<bool> =
        LazyLock::new(|| std::env::var("FLEA_TRACE_TABDRAG").as_deref() == Ok("1"));
    *ON
}

// Sample output: "TABDRAG drag-finished pid=101 action=0".
pub(crate) fn line(stage: &str, pid: u32, detail: &str) -> String {
    format!("TABDRAG {stage} pid={pid} {detail}")
}

/// Read once and silent otherwise; the detail is built only while the trace is on.
pub(crate) fn stage(stage: &str, detail: impl FnOnce() -> String) {
    if on() {
        eprintln!("{}", line(stage, std::process::id(), &detail()));
    }
}

/// Qt's DropAction number, which drag-finished reports: 0 ignored, 1 copy, 2 move.
pub(crate) fn action(action: Option<gpui::NativeDragAction>) -> u8 {
    match action {
        Some(gpui::NativeDragAction::Copy) => 1,
        Some(gpui::NativeDragAction::Move) => 2,
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    // EY-345: tests/ui.sh xwtab greps these exact shapes, as TabBar.qml printed them.
    #[test]
    fn ey345_stage_lines_read_as_the_qml_trace_did() {
        assert_eq!(
            super::line("drag-finished", 101, "action=0"),
            "TABDRAG drag-finished pid=101 action=0"
        );
        assert_eq!(super::action(None), 0);
        assert_eq!(super::action(Some(gpui::NativeDragAction::Copy)), 1);
        assert_eq!(super::action(Some(gpui::NativeDragAction::Move)), 2);
    }
}
