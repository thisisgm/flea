//! Preview host logic shared by the GPUI window and the TUI: kinds, facts, the swap, the settle and Compare.
pub mod ask;
pub mod compare;
pub mod facts;
pub mod ground;
pub mod keys;
pub mod kinds;
pub mod meta;
pub mod photo;
pub mod pixels;
pub mod property;
pub mod rect;
pub mod scroll;
pub mod settle;
pub mod sniff;
pub mod swap;
pub mod thumbs;

#[cfg(test)]
mod ask_tests;
#[cfg(test)]
mod compare_tests;
#[cfg(test)]
mod facts_tests;
#[cfg(test)]
mod ground_tests;
#[cfg(test)]
mod keys_tests;
#[cfg(test)]
mod photo_tests;
#[cfg(test)]
mod pixels_tests;
#[cfg(test)]
mod property_tests;
#[cfg(test)]
mod scroll_tests;
#[cfg(test)]
mod swap_tests;
#[cfg(test)]
mod thumbs_tests;

// The separator every one-line facts string joins on, a spaced middle dot.
pub const FACT_SEP: &str = " \u{b7} ";

// English from the compiled catalog alone, so a test reads no installed language or Omarchy data.
#[cfg(test)]
pub(crate) fn english() -> crate::i18n::I18n {
    let none = std::path::PathBuf::from("/nonexistent/flea-pv1");
    crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}
