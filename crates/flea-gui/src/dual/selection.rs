//! U26 cache guarantees on the canonical U03 input selection owner.
#[cfg(test)]
mod tests {
    use crate::window::locale::Snapshot;
    use crate::window::pane::PaneHost;
    use std::rc::Rc;
    use std::sync::Arc;

    fn host() -> PaneHost {
        let locale = Snapshot {
            revision: 1,
            catalog: Arc::new(crate::test_catalogs::english()),
        };
        PaneHost::new("/pane".into(), None, 620., &locale)
    }

    #[test]
    fn painted_selection_caches_per_host() {
        let mut left = host();
        let mut right = host();
        assert_eq!(left.input.selection.snapshot.clone(), None);
        left.selected = vec![1, 2, 3];
        left.input.selection.sync(&left.selected);
        assert_eq!(
            left.input.selection.snapshot.clone(),
            Some(Rc::from([1, 2, 3]))
        );
        // Same allocation while unchanged: no per-frame rebuild.
        let a = left.input.selection.snapshot.clone().unwrap();
        left.input.selection.sync(&left.selected);
        let b = left.input.selection.snapshot.clone().unwrap();
        assert!(Rc::ptr_eq(&a, &b), "a live selection keeps one snapshot");
        // The other side paints its own marks, never its sibling's.
        assert_eq!(right.input.selection.snapshot.clone(), None);
        right.selected = vec![7];
        right.input.selection.sync(&right.selected);
        assert_eq!(right.input.selection.snapshot.clone(), Some(Rc::from([7])));
        assert_eq!(
            left.input.selection.snapshot.clone(),
            Some(Rc::from([1, 2, 3]))
        );
        // Rebuild on change, drop on clear.
        left.selected = vec![5];
        left.input.selection.sync(&left.selected);
        assert_eq!(left.input.selection.snapshot.clone(), Some(Rc::from([5])));
        left.selected.clear();
        left.input.selection.sync(&left.selected);
        assert_eq!(left.input.selection.snapshot.clone(), None);
    }
}
