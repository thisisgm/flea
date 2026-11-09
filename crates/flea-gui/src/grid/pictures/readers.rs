//! The shared lender's raw source and decoder state, guarded by the current listing, facts and policy.
use super::{Map, Pictures, Slot};
use crate::list::pane::Held;
use flea::ui_logic_preview_core::{pixels::is_vector_name, thumbs::CACHE_SIZE};

#[derive(Clone, Copy)]
#[repr(i32)]
enum ImageStatus {
    Null = 0,
    Ready = 1,
    Loading = 2,
    Error = 3,
}

impl Map {
    fn current(&self, held: &Held, row: usize) -> Option<&Slot> {
        let facts = held.row(row)?;
        if self.listing != held.listing || !self.policy.is_some_and(|policy| policy.allowed(facts))
        {
            return None;
        }
        self.slots
            .get(&row)
            .filter(|slot| slot.identity().is_some_and(|identity| identity.is(facts)))
    }
}

impl Pictures {
    pub(crate) fn cache_only(&self) -> bool {
        let map = self.map.borrow();
        map.policy
            .zip(map.class_for(&map.path))
            .is_none_or(|(policy, class)| policy.cache_only(class))
    }

    /// Whether `row` holds its source's own pixels: a refinement, a cache-only picture, or a still raster's thumbnail under the cache edge.
    pub(crate) fn own_pixels(&self, row: usize) -> bool {
        if self.cache_only() {
            return true;
        }
        let map = self.map.borrow();
        let whole = match map.slots.get(&row) {
            Some(Slot::Ready {
                picture,
                image: true,
                ..
            }) => {
                let (w, h) = picture.decoded();
                !is_vector_name(&picture.identity.name) && f64::from(w.max(h)) < CACHE_SIZE
            }
            _ => false,
        };
        map.bases.contains_key(&row) || whole
    }
    pub(crate) fn trace(&self, row: usize) -> Option<(crate::preview_trace::Trace, bool)> {
        let map = self.map.borrow();
        Some((
            map.traces.get(&row)?.clone(),
            map.refinements
                .get(&row)
                .is_some_and(|refinement| refinement.ready),
        ))
    }

    pub(crate) fn quality_pixels(&self, row: usize) -> Option<((u32, u32), bool)> {
        let map = self.map.borrow();
        let refinement = map.refinements.get(&row)?;
        Some((refinement.requested.as_ref()?.size, refinement.ready))
    }

    /// Source decodes of the row's refined file, for the decodes-per-open reader.
    pub(crate) fn quality_decodes(&self, row: usize) -> usize {
        let map = self.map.borrow();
        map.refinements
            .get(&row)
            .and_then(|refinement| refinement.requested.as_ref())
            .map_or(0, |request| {
                crate::preview_quality::reuse::decodes(&request.path)
            })
    }

    pub(crate) fn file(&self, held: &Held, row: usize) -> String {
        let map = self.map.borrow();
        match map.current(held, row) {
            Some(
                Slot::Decoding { file, .. } | Slot::Ready { file, .. } | Slot::Failed { file, .. },
            ) => file.clone(),
            _ => String::new(),
        }
    }

    // Image.Ready on the row's current slot, the test ui/Ipc.qml rowThumbReady makes.
    pub(crate) fn ready(&self, held: &Held, row: usize) -> bool {
        self.status(held, row) == ImageStatus::Ready as i32
    }

    // Qt Image.Null, Image.Ready, Image.Loading and Image.Error are the frozen IPC values.
    pub(crate) fn status(&self, held: &Held, row: usize) -> i32 {
        (match self.map.borrow().current(held, row) {
            Some(Slot::Ready { .. }) => ImageStatus::Ready,
            Some(Slot::Decoding { .. }) => ImageStatus::Loading,
            Some(Slot::Failed { .. }) => ImageStatus::Error,
            _ => ImageStatus::Null,
        }) as i32
    }
}

impl Pictures {
    pub(crate) fn policy_files(
        &self,
        held: &Held,
        path: &str,
        policy: super::Policy,
    ) -> std::collections::BTreeMap<usize, Option<String>> {
        let map = self.map.borrow();
        if map.path != path {
            return Default::default();
        }
        map.order
            .iter()
            .filter_map(|&row| {
                let facts = held.row(row).filter(|facts| policy.allowed(facts))?;
                let slot = map.slots.get(&row)?;
                let terminal = match slot {
                    Slot::Refused { observed } | Slot::Missed { observed } => Some(observed),
                    _ => None,
                };
                if let Some(identity) = terminal {
                    if map.listing != held.listing
                        || !map.policy.is_some_and(|policy| policy.allowed(facts))
                        || !identity.is(facts)
                    {
                        return None;
                    }
                } else {
                    map.current(held, row)?;
                }
                let value = match slot {
                    Slot::Asked { cache: false, .. } => None,
                    Slot::Asked { cache: true, .. } => Some("cache-asked".into()),
                    Slot::Decoding { file, .. }
                    | Slot::Ready { file, .. }
                    | Slot::Failed { file, .. } => Some(file.clone()),
                    Slot::Refused { .. } => Some(String::new()),
                    Slot::Missed { .. } => Some("cache-missed".into()),
                };
                Some((row, value))
            })
            .collect()
    }
}
