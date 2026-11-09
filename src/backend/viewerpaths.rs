// Image paths from the standing name arena, with the same MIME table as the listing's rows.
use flea::bytename::ExactText as _;
use super::{listing::Listing, mime::Db};
use std::path::Path;

#[derive(Debug)]
pub struct Ask {
    pub start: usize,
    pub rows: Option<Vec<usize>>,
    pub selected: bool,
}

pub fn ordered(base: &Path, listing: &Listing, mime: &Db, ask: &Ask) -> Vec<String> {
    if ask.start >= listing.len() {
        return Vec::new();
    }
    let rows: Box<dyn Iterator<Item = usize>> = match &ask.rows {
        Some(scope) => {
            let mut rows = scope.clone();
            rows.sort_unstable();
            rows.dedup();
            // A filter is exactly its held subsequence; an explicit empty scope never expands to the folder.
            if !rows.contains(&ask.start) {
                return Vec::new();
            }
            if !ask.selected {
                let first = rows.partition_point(|row| *row < ask.start);
                rows.rotate_left(first);
            }
            Box::new(rows.into_iter())
        }
        None => Box::new((ask.start..listing.len()).chain(0..ask.start)),
    };
    let mut budget = crate::imageopen::Budget::new();
    let mut out = Vec::new();
    for row in rows {
        if row >= listing.len() || listing.is_dir(row) {
            continue;
        }
        let name = listing.name(row);
        if !mime
            .lookup(name)
            .is_some_and(|kind| kind.starts_with("image/"))
        {
            continue;
        }
        let path = base.join(name);
        if !budget.take(&path) {
            break;
        }
        out.push(path.exact_text().into_owned());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn listing() -> (Listing, Db) {
        let mut listing = Listing::new();
        for (name, directory) in [
            ("z.png", false),
            ("notes.txt", false),
            ("folder.jpg", true),
            ("middle.CR2", false),
            ("a.jpg", false),
        ] {
            listing.push(name, directory);
        }
        (listing, Db::from_str("50:image/png:*.png\n50:image/jpeg:*.jpg\n50:image/x-canon-cr2:*.cr2\n50:text/plain:*.txt\n"))
    }
    #[test]
    fn the_standing_sort_rotates_at_the_opened_image_without_touching_the_filesystem() {
        let (listing, mime) = listing();
        let out = ordered(
            Path::new("/does-not-exist"),
            &listing,
            &mime,
            &Ask {
                start: 3,
                rows: None,
                selected: false,
            },
        );
        assert_eq!(
            out,
            [
                "/does-not-exist/middle.CR2",
                "/does-not-exist/a.jpg",
                "/does-not-exist/z.png"
            ]
        );
    }
    #[test]
    fn filter_and_selection_keep_only_their_rows_in_listing_order() {
        let (listing, mime) = listing();
        let base = Path::new("/d");
        assert_eq!(
            ordered(
                base,
                &listing,
                &mime,
                &Ask {
                    start: 4,
                    rows: Some(vec![0, 4]),
                    selected: false
                }
            ),
            ["/d/a.jpg", "/d/z.png"]
        );
        assert_eq!(
            ordered(
                base,
                &listing,
                &mime,
                &Ask {
                    start: 4,
                    rows: Some(vec![4, 0, 4, 100]),
                    selected: true
                }
            ),
            ["/d/z.png", "/d/a.jpg"]
        );
        assert!(ordered(
            base,
            &listing,
            &mime,
            &Ask {
                start: 4,
                rows: Some(vec![]),
                selected: false
            }
        )
        .is_empty());
        assert!(ordered(
            base,
            &listing,
            &mime,
            &Ask {
                start: 100,
                rows: None,
                selected: false
            }
        )
        .is_empty());
    }
}
