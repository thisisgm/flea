//! The second pane's session: focus, settings patches, lazy start (no second backend or Element before Dual); leaving Dual keeps the host unrendered.

use flea::{json::escape, jsondoc::Json};

/// Only the focused pane draws the accent cursor; row focus supplies the keyboard-held grey.
pub fn pane_focused(side: usize, focus_side: usize, dual: bool) -> bool {
    if dual {
        side == focus_side
    } else {
        side == 0
    }
}

/// WindowBody.rememberDual: the pane's own focus save carries focus and paths, and must leave the stored split alone.
pub fn remember_patch(focus: usize, paths: Option<(&str, &str)>) -> String {
    match paths {
        Some((left, right)) => format!(
            "{{\"dual\":{{\"focus\":{},\"paths\":[\"{}\",\"{}\"]}}}}",
            focus,
            escape(left),
            escape(right)
        ),
        None => format!("{{\"dual\":{{\"focus\":{focus}}}}}"),
    }
}

/// One split leaf on divider release, so both listings re-lay from it.
pub fn split_patch(split: i64) -> String {
    format!("{{\"dual\":{{\"split\":{split}}}}}")
}

/// The stored split in ten-thousandths, 5000 when never saved; returned as saved, never clamped, since the clamp only lays out.
pub fn stored_split(data: &Json) -> i64 {
    data.get("dual")
        .and_then(|dual| dual.get("split"))
        .and_then(Json::as_f64)
        .filter(|split| split.is_finite() && *split >= 0.)
        .map(|split| split.round() as i64)
        .unwrap_or(super::layout::EQUAL_SPLIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_save_leaves_the_stored_split_alone() {
        let patch = remember_patch(1, Some(("/left", "/right")));
        assert!(patch.contains("\"focus\":1"), "focus lands, got {patch}");
        assert!(
            patch.contains("\"paths\":[\"/left\",\"/right\"]"),
            "paths land, got {patch}"
        );
        assert!(!patch.contains("split"), "split untouched, got {patch}");
        let bare = remember_patch(0, None);
        assert_eq!(bare, "{\"dual\":{\"focus\":0}}");
    }

    #[test]
    fn patches_escape_paths() {
        let patch = remember_patch(0, Some(("/a\"b", "/c")));
        assert!(patch.contains("/a\\\"b"), "escaped, got {patch}");
    }

    #[test]
    fn split_patch_carries_one_leaf() {
        assert_eq!(split_patch(6_101), "{\"dual\":{\"split\":6101}}");
    }

    #[test]
    fn stored_split_defaults_and_passes_through() {
        let empty = flea::jsondoc::parse("{}").unwrap();
        assert_eq!(stored_split(&empty), 5_000);
        for (body, want) in [
            ("{\"dual\":{\"split\":100}}", 100),
            ("{\"dual\":{\"split\":6500}}", 6_500),
        ] {
            let data = flea::jsondoc::parse(body).unwrap();
            assert_eq!(stored_split(&data), want, "never clamped: {body}");
        }
    }

    #[test]
    fn only_the_focused_dual_side_draws_accent() {
        assert!(pane_focused(0, 0, true));
        assert!(!pane_focused(1, 0, true));
        assert!(!pane_focused(0, 1, true));
        assert!(pane_focused(1, 1, true));
        assert!(pane_focused(0, 0, false));
        assert!(!pane_focused(1, 0, false));
    }
}
