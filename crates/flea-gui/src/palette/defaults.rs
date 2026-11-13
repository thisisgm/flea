//! Cache the default theme without allocating during a frame.
use super::*;

impl Global for Theme {}

impl Default for Theme {
    fn default() -> Self {
        // Resolved once: a frame without an installed theme reads the default and must not allocate.
        static DEFAULT: std::sync::LazyLock<Theme> =
            std::sync::LazyLock::new(|| Theme::from_text(""));
        *DEFAULT
    }
}
