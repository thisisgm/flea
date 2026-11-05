//! Preview settings shared by the GUI and the terminal interface.
use super::Rule;

pub const PREVIEW: &[(&str, Rule)] = &[
    ("column", Rule::Bool),
    ("loadOn", Rule::Word(&["automatic", "manual"])),
    ("thumbnails", Rule::Word(&["off", "images", "media"])),
    (
        "thumbSize",
        Rule::Word(&["small", "medium", "large", "xlarge", "huge", "largest"]),
    ),
    // ExtThumbs: one switch per storage class beside the segment they depend on.
    ("thumbNetwork", Rule::Bool),
    ("thumbPhone", Rule::Bool),
    ("thumbUsb", Rule::Bool),
    ("prefetchOnHover", Rule::Bool),
    ("ctrlZoom", Rule::Bool),
    ("hideFilmstrip", Rule::Bool),
    ("preloadAdjacent", Rule::Bool),
    ("mediaControls", Rule::Bool),
    ("infoPanel", Rule::Bool),
    ("hideNavbarFullscreen", Rule::Bool),
    ("navbarHideMs", Rule::Count(1000.0, 10000.0)),
    ("volume", Rule::Count(0.0, 100.0)),
    ("width", Rule::Count(0.0, 100.0)),
    // Columns: each column's share of the room left of the preview, left to right, [] for equal columns.
    ("tracks", Rule::Tracks),
    // Quick Look's w: wraps long lines in a code or text file; off until pressed.
    ("wrap", Rule::Bool),
];
