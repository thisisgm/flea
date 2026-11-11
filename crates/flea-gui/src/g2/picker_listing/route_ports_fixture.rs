use super::*;

pub(super) fn landed_two() -> Listing {
    let mut list = Listing::default();
    list.open("/d");
    let gen = list.generation();
    assert!(list.landed(gen, 2));
    assert!(list.held_rows(
        gen,
        0,
        vec![
            RowFacts {
                source: None,
                depth: 0,
                open: false,
                name: "Docs".into(),
                dir: true,
                size: 0,
                mtime: 0,
                mode: 0o040755,
                icon: String::new(),
                link: None,
                real_modes: true,
                thumbless: false,
                kind: String::new(),
                inline: None,
                tags: None,
            },
            RowFacts {
                source: None,
                depth: 0,
                open: false,
                name: "a.txt".into(),
                dir: false,
                size: 0,
                mtime: 0,
                mode: 0o100644,
                icon: String::new(),
                link: None,
                real_modes: true,
                thumbless: false,
                kind: String::new(),
                inline: None,
                tags: None,
            },
        ]
    ));
    list
}
