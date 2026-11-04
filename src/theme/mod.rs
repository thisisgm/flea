// Shared theme core, kept independent of the drawing layer and of external crates.

mod color;
mod export;
mod material;
mod parse;
mod style;
mod tokens;
mod watch;

pub use color::{ensure_ratio, ratio, Color};
pub use material::Material;
pub use parse::{parse_colors, parse_shell, Values};
pub use style::{Decoration, Font, Spacing};
pub use tokens::{TagTheme, Theme, ThemeColors, TAG_NAMES, TAG_ROLES};
pub use watch::{FileStamp, ThemePaths, ThemeWatcher};

#[cfg(test)]
mod golden_tests;
#[cfg(test)]
mod review_r2_tests;
#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod tests;
