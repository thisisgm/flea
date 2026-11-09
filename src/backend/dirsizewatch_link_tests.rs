// Hard-linked files against the size cache, through the same Rig the parent's tests drive.
use super::*;

// Soak S7A-1: a write through a name outside the folder fires none of its watches, so that folder is never kept.
#[test]
fn a_folder_with_a_file_linked_from_outside_is_walked_on_every_ask() {
    const GROWTH_BYTES: usize = 1 << 20;
    let d = TestDir::new("dirsize-hardlink");
    let fx = d.dir("fx");
    let album = d.dir("fx/album");
    let clip = d.file("fx/album/clip.bin", "x");
    let outside = d.join("elsewhere.bin");
    std::fs::hard_link(&clip, &outside).unwrap();
    let mut rig = Rig::new(&d);
    rig.list(&fx, "album", &[]);
    let (bytes, walked) = rig.size();
    assert!(walked, "the first ask walks");
    assert_eq!(
        rig.st.settled.get(&album),
        None,
        "a folder with a file named outside keeps no size"
    );
    let mut out = std::fs::OpenOptions::new()
        .append(true)
        .open(&outside)
        .unwrap();
    out.write_all(&[0u8; GROWTH_BYTES]).unwrap();
    drop(out);
    rig.list(&fx, "album", &[]);
    assert_eq!(
        rig.size(),
        (bytes + GROWTH_BYTES as u64, true),
        "the re-list walks and counts the write through the outside name"
    );

    // Negative control: once the other name is gone, the folder is kept again.
    std::fs::remove_file(&outside).unwrap();
    rig.list(&fx, "album", &[]);
    let (bytes, walked) = rig.size();
    assert!(walked);
    // A fresh listing, so the answer comes from the settled cache and not the listing's own answered rows.
    rig.list(&fx, "album", &[]);
    assert_eq!(
        rig.size(),
        (bytes, false),
        "a folder of single-name files answers its settled size"
    );
}

// Every name of the linked file lies inside the folder, so every write fires one of its watches and the size is kept.
#[test]
fn a_folder_holding_every_name_of_a_linked_file_is_kept() {
    let d = TestDir::new("dirsize-hardlink-inside");
    let fx = d.dir("fx");
    let album = d.dir("fx/album");
    let clip = d.file("fx/album/clip.bin", "x");
    std::fs::create_dir(album.join("copies")).unwrap();
    std::fs::hard_link(&clip, album.join("copies/clip.bin")).unwrap();
    let mut rig = Rig::new(&d);
    rig.list(&fx, "album", &[]);
    // The walk counts each name's own size, so a write grows the total once per name.
    const WRITTEN: &[u8] = b"12345";
    let (bytes, walked) = rig.size();
    assert!(walked, "the first ask walks");
    rig.list(&fx, "album", &[]);
    assert_eq!(
        rig.size(),
        (bytes, false),
        "both names counted, so a fresh listing answers the settled size"
    );
    let mut out = std::fs::OpenOptions::new()
        .append(true)
        .open(album.join("copies/clip.bin"))
        .unwrap();
    out.write_all(WRITTEN).unwrap();
    drop(out);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while rig.st.settled.get(&album).is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "the write through the second inside name never reached the size watch"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    rig.list(&fx, "album", &[]);
    assert_eq!(
        rig.size(),
        (bytes + 2 * WRITTEN.len() as u64, true),
        "the next ask walks and counts the write"
    );
}
