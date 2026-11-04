// EY-484: contents results rank by distance from the searched folder, then by name, as CHANGES.md search-ranked says.
use super::Fixture;

#[test]
fn fixlogic_ey484_the_searched_folders_own_hit_leads_then_nearer_then_by_name() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let mut f = Fixture::new();
    for folder in ["Code", "Docs", "Media", "Code/deep"] {
        std::fs::create_dir_all(f.root.join(folder)).unwrap();
    }
    for path in [
        "notes.txt",
        "Code/main.rs",
        "Docs/notes.txt",
        "Media/main.rs",
        "Code/deep/a.txt",
    ] {
        std::fs::write(f.root.join(path), "needle\n").unwrap();
    }
    f.search(false, 1, 10);
    f.finish();
    let order: Vec<String> = (0..f.st.listing.len())
        .map(|row| f.st.listing.name(row).to_owned())
        .collect();
    assert_eq!(
        order,
        [
            "notes.txt",
            "Code/main.rs",
            "Media/main.rs",
            "Docs/notes.txt",
            "Code/deep/a.txt"
        ]
    );
}
