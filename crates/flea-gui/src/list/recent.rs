//! The bounded Recent metadata retained with the listing it describes.
use flea::ui_logic_pl4::recentmode::Visits;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Rows {
    pub visits: Visits,
    pub home: String,
}
