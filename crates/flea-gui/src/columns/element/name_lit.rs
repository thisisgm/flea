//! When a Columns row draws its name at full contrast, so a hidden name fades only at rest.
use super::super::geometry::Slot;
use super::super::paint::Focus;

impl Focus {
    /// The cursor, a lifted path row, a drop target, a marked row in the active track and the lit pointer row, as List
    /// lifts them.
    pub(crate) fn name_lit(&self, slot: Slot, index: usize, lifted: bool) -> bool {
        let active = slot == Slot::Active;
        (active && self.cursor == Some(index))
            || lifted
            || self.dropping(slot, index)
            || (active && self.selected.binary_search(&index).is_ok())
            || self.hover_lit(slot, index)
    }
}

#[cfg(test)]
mod tests {
    use super::{Focus, Slot};

    fn focus() -> Focus {
        Focus {
            cursor: Some(1),
            selected: [3].into(),
            hover: Some((Slot::Active, 4)),
            hover_wash: true,
            drop: Some((Slot::Active, 5, "Move".into())),
            ..Focus::default()
        }
    }

    #[test]
    fn hidden_names_brighten_in_every_lit_state_and_rest_otherwise() {
        let focus = focus();
        assert!(!focus.name_lit(Slot::Active, 0, false), "at rest");
        assert!(focus.name_lit(Slot::Active, 1, false), "cursor");
        assert!(focus.name_lit(Slot::Parent, 2, true), "lifted path folder");
        assert!(focus.name_lit(Slot::Active, 3, false), "selected");
        assert!(focus.name_lit(Slot::Active, 4, false), "hovered");
        assert!(focus.name_lit(Slot::Active, 5, false), "drop target");
    }

    #[test]
    fn selection_and_cursor_light_only_the_active_track_and_hover_needs_the_setting() {
        let mut focus = focus();
        assert!(
            !focus.name_lit(Slot::Parent, 1, false),
            "cursor index elsewhere"
        );
        assert!(
            !focus.name_lit(Slot::Child, 3, false),
            "selection index elsewhere"
        );
        focus.hover_wash = false;
        assert!(
            !focus.name_lit(Slot::Active, 4, false),
            "hover without the setting"
        );
    }
}
