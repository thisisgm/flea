//! Completed pictures remain reusable across query generations in the same folder.
use super::{Map, Slot};
use crate::list::pane::Held;
use std::collections::{hash_map::Entry, HashMap, VecDeque};

impl Map {
    pub(super) fn follow(&mut self, held: &Held, path: &str) {
        let mut kept = VecDeque::new();
        if path == self.path {
            kept = std::mem::take(&mut self.carried);
            for row in std::mem::take(&mut self.order) {
                if let Some(slot @ Slot::Ready { .. }) = self.slots.remove(&row) {
                    kept.push_back(slot);
                }
            }
        } else {
            self.names.clear();
        }
        self.clear();
        self.listing = held.listing;
        self.path = path.to_owned();
        self.restore_records(held, kept);
        self.sweep_faded();
    }

    pub(super) fn restore(&mut self, held: &Held) {
        if !self.carried.is_empty() {
            let kept = std::mem::take(&mut self.carried);
            self.restore_records(held, kept);
        }
    }

    fn restore_records(&mut self, held: &Held, kept: VecDeque<Slot>) {
        if kept.is_empty() {
            return;
        }
        let at: HashMap<&str, usize> = held
            .rows
            .iter()
            .enumerate()
            .map(|(offset, facts)| (facts.name.as_str(), held.index_at(offset)))
            .collect();
        for slot in kept {
            let Slot::Ready { ref picture, .. } = slot else {
                continue;
            };
            let destination = at.get(picture.identity.name.as_str()).copied();
            if destination.is_some_and(|row| {
                held.row(row)
                    .is_none_or(|facts| !picture.identity.is(facts))
            }) {
                self.retire(picture);
                continue;
            }
            if let Some(row) = destination {
                if let Entry::Vacant(entry) = self.slots.entry(row) {
                    self.order.push_back(row);
                    entry.insert(slot);
                    continue;
                }
            }
            self.carried.push_back(slot);
        }
    }

    pub(super) fn evict_carried(&mut self) -> bool {
        let Some(slot) = self.carried.pop_front() else {
            return false;
        };
        if let Slot::Ready { picture, .. } = slot {
            self.retire(&picture);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::element::{Identity, PictureKind, Thumbnail};
    use flea::protocol::RowFacts;
    use gpui::RenderImage;
    use std::sync::Arc;

    fn picture(name: &str) -> Slot {
        Slot::Ready {
            file: String::new(),
            image: true,
            picture: Arc::new(Thumbnail {
                identity: Identity {
                    name: name.into(),
                    size: 1,
                    mtime: 1,
                },
                kind: PictureKind::Image,
                image: Arc::new(RenderImage::new(vec![image::Frame::new(
                    image::RgbaImage::new(8, 8),
                )])),
            }),
        }
    }

    fn held(listing: usize, name: &str) -> Held {
        Held {
            listing,
            total: 1,
            start: 0,
            rows: vec![RowFacts {
                name: name.into(),
                size: 1,
                mtime: 1,
                icon: "image-x-generic".into(),
                mode: 0o100644,
                ..Default::default()
            }],
        }
    }

    fn cached() -> Map {
        let mut map = Map {
            listing: 1,
            path: "/files".into(),
            ..Default::default()
        };
        map.remember(0, picture("a.png"));
        map.follow(&held(2, "b.png"), "/files");
        assert_eq!(map.carried.len(), 1);
        map
    }

    #[test]
    fn fx477_cached_picture_rejects_changed_facts_and_leaving_the_folder_releases_it() {
        let mut map = cached();
        let mut changed = held(3, "a.png");
        changed.rows[0].size = 2;
        map.follow(&changed, "/files");
        assert!(map.slots.is_empty() && map.carried.is_empty());
        assert_eq!(map.counters.retired, 1);
        let mut map = cached();
        map.follow(&held(3, "a.png"), "/else");
        assert!(map.slots.is_empty() && map.carried.is_empty());
        assert_eq!(map.counters.retired, 1);
    }

    #[test]
    fn fx477_cached_picture_restores_when_its_facts_arrive_and_release_drops_it() {
        let mut map = cached();
        map.restore(&held(2, "a.png"));
        assert!(matches!(map.slots.get(&0), Some(Slot::Ready { .. })));
        assert!(map.carried.is_empty());
        map.follow(&held(3, "b.png"), "/files");
        map.release();
        assert!(map.slots.is_empty() && map.carried.is_empty());
        assert_eq!(map.counters.retired, 1);
    }

    #[test]
    fn fx477_cached_pictures_share_the_record_cap_and_pixel_budget() {
        let mut map = Map {
            listing: 1,
            path: "/files".into(),
            ..Default::default()
        };
        for listing in 2..=super::super::CAP + 6 {
            map.remember(0, picture(&format!("{listing}.png")));
            map.follow(&held(listing, "absent.png"), "/files");
            map.trim();
            assert!(map.order.len() + map.carried.len() <= super::super::CAP);
        }
        assert_eq!(map.carried.len(), super::super::CAP);
        let mut map = cached();
        map.remember(0, picture("b.png"));
        super::super::quality::evict(&mut map, &[0], 64);
        assert!(map.carried.is_empty());
        assert!(matches!(map.slots.get(&0), Some(Slot::Ready { .. })));
    }
}
