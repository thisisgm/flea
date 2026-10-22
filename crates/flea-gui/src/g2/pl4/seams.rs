//! Plain-data stand-ins for contracts other units own, wired at the join.
use super::content::DrawnSection;

/// G1.11 rail host: takes the drawn sections whenever their revision moves and owns geometry, reveal and paint.
pub trait RailContentHost {
    fn sections_changed(&mut self, revision: u64, sections: &[DrawnSection]);
}

/// G0.5 theme tokens: the "#rrggbb" roles a tag reads and its two grounds; absent until they land, so tags draw the framed digit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeRoles {
    pub roles: Vec<(String, String)>,
    pub background: String,
    pub surface: String,
}

impl ThemeRoles {
    pub fn role(&self, name: &str) -> Option<String> {
        self.roles
            .iter()
            .find(|(role, _)| role == name)
            .map(|(_, hex)| hex.clone())
    }
}
