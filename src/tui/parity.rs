pub fn refusal_text(action: &str) -> super::words::Text {
    if let Some(reason) = action.strip_prefix("unsupported:") {
        use super::words::Text;
        let field = match reason { "detail:size" => Some("window.column_size"), "detail:mode" => Some("list.column_mode"),
            "detail:modified" => Some("window.column_modified"), "detail:owner" => Some("properties.owner"), _ => None };
        if let Some(field) = field {
            return Text::Joined(vec![Text::Key("tui.parity.this_action_is_unavailable_in_the_terminal_interface"), "; I: ".into(), super::words::args("tui.frame.detail", vec![Text::Key(field)])]);
        }
        if reason == "Specific detail modes are unavailable in this terminal build" {
            return Text::Joined(vec![reason.into(), "; I: ".into(), Text::Key("keys.detailNext")]);
        }
        return reason.into();
    }
    let key = match action {
        "viewList" | "viewColumns" | "viewGrid" | "viewDual" => "tui.parity.alternate_listing_views_are_available_in_the_flea_window",
        "sidebar" => "tui.parity.the_terminal_interface_has_no_sidebar",
        "textSizeUp" | "textSizeDown" | "textSizeReset" => "tui.parity.change_text_size_in_your_terminal_settings",
        "settings" => "tui.parity.settings_are_available_in_the_flea_window",
        "addNetwork" => "tui.parity.add_network_places_in_the_flea_window",
        "eject" => "tui.parity.unmount_volumes_in_the_flea_window",
        "tags" => "tui.parity.tags_are_unavailable_in_this_terminal_build",
        "copyAs" | "copyPath" | "copyName" | "copyStem" | "copyUri" | "copydirpath" | "copyQuoted" => "tui.parity.copy_as_text_is_unavailable_in_this_terminal_build",
        "pasteAs" | "pasteLink" | "pasteAbsoluteLink" | "pasteHardLink" => "tui.parity.paste_links_are_unavailable_in_this_terminal_build",
        "zoomOut" | "zoomIn" | "zoomFit" | "expand" => "tui.parity.focus_a_supported_preview_to_use_its_controls",
        "markdownView" | "toggleWrap" | "copyPatch" => "tui.parity.this_preview_control_is_unavailable_in_this_terminal_build",
        _ => "tui.parity.this_action_is_unavailable_in_the_terminal_interface",
    };
    super::words::Text::Key(key)
}

#[cfg(test)]
#[path = "parity_tests.rs"]
mod tests;
