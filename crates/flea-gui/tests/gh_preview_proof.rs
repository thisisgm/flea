//! GH-PREVIEW-PROOF #347: the real Poppler render thread under 200 replacements across 20 documents.
use flea_gui::g2::pv2::poppler;
use flea_gui::g2::pv2::render::{DocId, Renderer, Reply};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

#[allow(dead_code)]
#[path = "g2_pv2/fixture.rs"]
mod fixture;

struct Root(PathBuf);

impl Drop for Root {
    fn drop(&mut self) {
        let ours = self.0.is_absolute()
            && self.0.join(".gh-preview-proof").is_file()
            && self
                .0
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("run."));
        if ours && !std::thread::panicking() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn root() -> Root {
    let parent =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/gh-preview-proof-run");
    std::fs::create_dir_all(&parent).unwrap();
    let parent = parent.canonicalize().unwrap();
    let out = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("run.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(out.status.success());
    let path = PathBuf::from(String::from_utf8(out.stdout).unwrap().trim());
    assert!(path.is_absolute() && path.parent() == Some(parent.as_path()));
    std::fs::write(path.join(".gh-preview-proof"), "gh-preview-proof\n").unwrap();
    Root(path)
}

#[test]
fn gh347_render_thread_200_replacements_across_20_pdfs_draw_or_cancel_every_page() {
    assert_eq!(
        poppler::available(),
        Ok(()),
        "BLOCKED-HOST: libpoppler-glib must load in this process for the real-renderer stress"
    );
    // The receipt that an ASan run was instrumented: the sanitizer runtime is linked into this binary.
    let shadow = std::fs::read_to_string("/proc/self/maps")
        .is_ok_and(|maps| maps.lines().any(|l| l.starts_with("7fff8000-")));
    let linked = std::fs::read("/proc/self/exe")
        .is_ok_and(|exe| exe.windows(19).any(|w| w == b"__asan_report_load8"));
    let asan = shadow || linked;
    eprintln!("gh347 renderer asan shadow_mapped={shadow} runtime_linked={linked}");
    eprintln!(
        "gh347 renderer asan_runtime={asan} ASAN_OPTIONS={:?}",
        std::env::var("ASAN_OPTIONS").ok()
    );
    let root = root();
    let pages = [fixture::LETTER, fixture::LANDSCAPE, fixture::TURNED];
    let paths: Vec<PathBuf> = (0..20)
        .map(|i| {
            let path = root.0.join(format!("doc-{i:02}.pdf"));
            std::fs::write(&path, fixture::pdf(&pages)).unwrap();
            path
        })
        .collect();
    let renderer = Renderer::spawn();
    let mut asked: HashMap<DocId, HashSet<usize>> = HashMap::new();
    let mut opened_docs = Vec::new();
    let mut current: Option<DocId> = None;
    let mut rendered: HashMap<DocId, HashSet<usize>> = HashMap::new();
    let mut opened: HashSet<DocId> = HashSet::new();
    let mut failures = Vec::new();
    let mut unlinked = 0;
    let mut seed = 0x2545_f491u32;
    // One reply into the receipts; false when none was waiting.
    let take = |asked: &HashMap<DocId, HashSet<usize>>,
                rendered: &mut HashMap<DocId, HashSet<usize>>,
                opened: &mut HashSet<DocId>,
                failures: &mut Vec<String>| {
        match renderer.replies.try_recv() {
            Ok(Reply::Opened { doc, sizes, .. }) => {
                assert_eq!(sizes.len(), 3, "doc {doc} reads its three page sizes");
                opened.insert(doc);
            }
            Ok(Reply::Rendered {
                doc, page, image, ..
            }) => {
                assert!(
                    image.size(0).width.0 > 0,
                    "doc {doc} page {page} has pixels"
                );
                assert!(
                    asked[&doc].contains(&page),
                    "doc {doc} page {page} was asked"
                );
                assert!(
                    rendered.entry(doc).or_default().insert(page),
                    "doc {doc} page {page} renders once"
                );
            }
            Ok(Reply::Sizes { .. }) => {}
            Ok(Reply::OpenFailed { doc, .. }) => failures.push(format!("open {doc}")),
            Ok(Reply::RenderFailed { doc, page, .. }) => {
                failures.push(format!("render {doc}:{page}"))
            }
            Err(_) => return false,
        }
        true
    };
    for step in 0..200 {
        let path = paths[step % 20].clone();
        if let Some(old) = current.take() {
            renderer.close(old);
        }
        // Quick Look opens paged rather than continuous, so its open answers every page size.
        let doc = renderer.open(path.clone(), false);
        asked.insert(doc, (0..3).collect());
        // Once a document is open its bytes are the renderer's: the name is unlinked and rewritten before its renders.
        if step % 20 == 7 {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !opened.contains(&doc) && !failures.iter().any(|f| f == &format!("open {doc}")) {
                assert!(Instant::now() < deadline, "doc {doc} opens within 30 s");
                if !take(&asked, &mut rendered, &mut opened, &mut failures) {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            std::fs::remove_file(&path).unwrap();
            std::fs::write(&path, b"not a pdf any more").unwrap();
            unlinked += 1;
        }
        for page in 0..3 {
            renderer.render(doc, page, 300 + (step as u32 % 5) * 40);
        }
        opened_docs.push(doc);
        current = Some(doc);
        if step % 20 == 7 {
            // The replaced name holds garbage now; the open document must still draw all three pages from its own bytes.
            let deadline = Instant::now() + Duration::from_secs(30);
            while rendered.get(&doc).map_or(0, HashSet::len) < 3 {
                assert!(
                    Instant::now() < deadline,
                    "unlinked doc {doc} draws its pages within 30 s: {failures:?}"
                );
                if !take(&asked, &mut rendered, &mut opened, &mut failures) {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            std::fs::write(&path, fixture::pdf(&pages)).unwrap();
        }
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        std::thread::sleep(Duration::from_micros(u64::from(seed % 4000)));
        while take(&asked, &mut rendered, &mut opened, &mut failures) {}
    }
    let last = current.unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    while rendered.get(&last).map_or(0, HashSet::len) < 3 {
        assert!(
            Instant::now() < deadline,
            "the final document's pages land within 120 s"
        );
        if !take(&asked, &mut rendered, &mut opened, &mut failures) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    assert_eq!(unlinked, 10, "ten documents lost their name while open");
    let drawn: usize = rendered.values().map(HashSet::len).sum();
    let cancelled = 200 * 3 - drawn;
    eprintln!(
        "gh347 renderer docs=200 opened={} drawn_pages={drawn} cancelled_pages={cancelled} failures={failures:?}",
        opened.len()
    );
    assert!(failures.is_empty(), "nothing fails: {failures:?}");
    assert!(opened.contains(&last), "the final document opened");
    for doc in &opened_docs {
        let got = rendered.get(doc).map_or(0, HashSet::len);
        assert!(got <= 3, "doc {doc} rendered at most what was asked");
        assert!(
            got == 0 || opened.contains(doc),
            "doc {doc} rendered only after its open"
        );
    }
}
