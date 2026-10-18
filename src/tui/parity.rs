pub fn refusal(action: &str) -> &str {
    if let Some(reason) = action.strip_prefix("unsupported:") { return reason; }
    match action {
        "detailNext" => "The detail column is unavailable in this build",
        "print" => "Printing is unavailable in this build",
        "searchContents" => "Content search is unavailable in this build",
        "viewList" | "viewColumns" | "viewGrid" | "viewDual" => "Alternate listing views are available in the Flea window",
        "sidebar" => "The terminal interface has no sidebar",
        "textSizeUp" | "textSizeDown" | "textSizeReset" => "Change text size in your terminal settings",
        "settings" => "Settings are available in the Flea window",
        "addNetwork" => "Add network places in the Flea window",
        "eject" => "Unmount volumes in the Flea window",
        "tags" => "Tags are unavailable in this terminal build",
        "copyAs" | "copyPath" | "copyName" | "copyStem" | "copyUri" | "copydirpath" | "copyQuoted" => "Copy as text is unavailable in this terminal build",
        "pasteAs" | "pasteLink" | "pasteAbsoluteLink" | "pasteHardLink" => "Paste links are unavailable in this terminal build",
        "zoomOut" | "zoomIn" | "zoomFit" | "expand" => "Focus a supported preview to use its controls",
        "markdownView" | "toggleWrap" | "copyPatch" => "This preview control is unavailable in this terminal build",
        _ => "This action is unavailable in the terminal interface",
    }
}

#[cfg(test)]
#[path = "parity_tests.rs"]
mod tests;
