//! Board-state frames per paint of each mounted surface under Sunset; unlisted surfaces have none.
use super::frame_gate::States;

// Columns: foreground, accent_frame, error_on_surface, disabled foreground, disabled muted_frame.
const fn row(
    foreground: usize,
    accent_frame: usize,
    error_on_surface: usize,
    disabled_foreground: usize,
    disabled_muted: usize,
) -> States {
    States {
        foreground,
        accent_frame,
        error_on_surface,
        disabled_foreground,
        disabled_muted,
    }
}

pub(crate) const CENSUS: &[(&str, States)] = &[
    ("bulk-rename", row(0, 4, 0, 0, 4)),
    ("chrome-focus-ring", row(4, 0, 0, 0, 0)),
    ("collision", row(1, 0, 0, 0, 0)),
    ("combine", row(13, 0, 0, 4, 12)),
    ("compress-password", row(8, 8, 0, 0, 0)),
    ("convert", row(0, 1, 0, 0, 0)),
    ("copy-to-card-field-buttons", row(0, 5, 0, 0, 0)),
    ("delete-permanently", row(1, 0, 0, 0, 0)),
    ("empty-trash", row(1, 0, 0, 0, 0)),
    ("extract-password", row(0, 4, 0, 0, 4)),
    ("fat-names", row(1, 0, 0, 0, 0)),
    ("keyboard-sheet-and-query", row(0, 4, 0, 0, 0)),
    ("move-to", row(0, 5, 0, 0, 0)),
    ("network-card-protocols-fields-buttons", row(2, 2, 0, 0, 0)),
    ("new-file", row(0, 5, 0, 0, 0)),
    ("open-with", row(0, 8, 0, 0, 0)),
    (
        "operation-fields-checkbox-ring-and-names",
        row(12, 12, 0, 0, 0),
    ),
    ("path-entry-and-jump-popup", row(0, 1, 0, 0, 0)),
    ("permissions", row(20, 0, 0, 0, 0)),
    ("plugins-add-card-field-buttons", row(16, 4, 0, 0, 4)),
    ("plugins-checkboxes", row(16, 0, 0, 0, 0)),
    ("plugins-choice-chips", row(8, 0, 0, 0, 0)),
    ("plugins-remove-card-buttons", row(8, 4, 0, 0, 0)),
    ("print", row(0, 0, 0, 0, 20)),
    ("properties", row(4, 0, 0, 0, 0)),
    ("rail-rename-field", row(0, 1, 0, 0, 0)),
    ("run-card", row(4, 0, 0, 0, 0)),
    ("settings-about", row(4, 0, 0, 0, 4)),
    ("settings-card-chips-checkboxes", row(4, 0, 0, 0, 0)),
    ("settings-display", row(4, 0, 0, 0, 0)),
    ("settings-keys", row(4, 0, 0, 0, 0)),
    ("settings-menus", row(68, 0, 0, 0, 0)),
    ("settings-places", row(28, 0, 0, 0, 0)),
    // 0401-settings-wide-b: the wider pane fits Thumbnail size's chips, adding its chosen foreground frame (4 quads).
    ("settings-preview", row(24, 0, 0, 0, 0)),
    ("settings-search-field", row(0, 4, 0, 0, 0)),
    // 0401-settings-view-c + 0401-settings-hover: Omi and Rotate add 4 each; Start in and New tabs open in fall below the fixture (-8).
    ("settings-view", row(36, 0, 0, 0, 4)),
    ("trash-strip-button", row(0, 0, 0, 0, 4)),
    ("unlock-card-password-buttons", row(4, 0, 0, 0, 0)),
    ("unlock-pointer-wash", row(4, 0, 0, 0, 0)),
];
