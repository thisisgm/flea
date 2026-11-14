use super::*;
use crate::backend::testdir::TestDir;
use std::cell::{Cell, RefCell};
use std::time::SystemTime;

const TYPES: &str = "50:font/ttf:*.ttf\n50:font/otf:*.otf\n50:font/collection:*.ttc\n50:font/woff:*.woff\n50:text/plain:*.txt\n";
const ALIASES: &str = "application/x-font-ttf font/ttf\napplication/x-font-otf font/otf\n";
const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];

fn ask(path: &Path) -> Ask {
    Ask {
        id: 4,
        path: path.to_path_buf(),
        fg: "#c0caf5".into(),
        bg: "#1a1b26".into(),
        sizes: vec![33, 22, 18],
        text: "The quick brown fox jumps over the lazy dog".into(),
    }
}

// A font file the tests can name: the tables are fontinfo's own tests, so only its name and bytes matter here.
fn font(d: &TestDir, name: &str) -> PathBuf {
    d.file(name, "not read by the stub")
}

struct Rig {
    jobs: Cell<usize>,
    seen: RefCell<Vec<Vec<String>>>,
    mime: Db,
    aliases: Aliases,
}

impl Rig {
    fn new() -> Rig {
        Rig {
            jobs: Cell::new(0),
            seen: RefCell::new(Vec::new()),
            mime: Db::from_str(TYPES),
            aliases: Aliases::from_str(ALIASES),
        }
    }

    // The stub renderer: records its argv and draws the three PNGs where the argv names them.
    fn run(&self, ask: &Ask, root: &Path) -> String {
        answer(
            ask,
            &self.mime,
            &self.aliases,
            root,
            "magick",
            &mut |inner, _font, work| {
                self.jobs.set(self.jobs.get() + 1);
                self.seen.borrow_mut().push(inner.to_vec());
                for i in 0..SAMPLES {
                    std::fs::write(work.dir.join(format!("{}.png", i)), PNG).expect("stub draws");
                }
                Ok(())
            },
        )
    }
}

fn files_of(line: &str) -> Vec<String> {
    crate::json::field_str_array(line, "files")
}

#[test]
fn the_argv_carries_the_font_the_colours_the_sizes_and_the_text_in_one_run() {
    let d = TestDir::new("fontargv");
    let out = vec![d.join("0.png"), d.join("1.png"), d.join("2.png")];
    let got = argv(
        "magick",
        Path::new("/f/Inter-Bold.ttf"),
        &ask(Path::new("/f/Inter-Bold.ttf")),
        "The quick brown fox",
        &out,
    );
    let want: Vec<String> = [
        "magick",
        "-define",
        "filename:literal=true",
        "-background",
        "#1a1b26",
        "-fill",
        "#c0caf5",
        "-font",
        "/f/Inter-Bold.ttf",
        "-pointsize",
        "33",
        "label:The quick brown fox",
        "-write",
        out[0].to_str().unwrap(),
        "+delete",
        "-pointsize",
        "22",
        "label:The quick brown fox",
        "-write",
        out[1].to_str().unwrap(),
        "+delete",
        "-pointsize",
        "18",
        "label:The quick brown fox",
        out[2].to_str().unwrap(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(got, want);
}

#[test]
fn the_text_and_its_percent_and_at_sign_are_closed_before_label_reads_them() {
    assert_eq!(label_text("100% sure").unwrap(), "100%% sure");
    for bad in [
        "",
        "@/etc/passwd",
        "a\\nb",
        "a\nb",
        &"x".repeat(MAX_TEXT_BYTES + 1),
    ] {
        assert!(label_text(bad).is_err(), "{:?} must be refused", bad);
    }
}

#[test]
fn a_miss_runs_one_job_and_a_hit_runs_none() {
    let d = TestDir::new("fontcache");
    let rig = Rig::new();
    let path = font(&d, "Inter-Bold.ttf");
    let root = d.join("cache");
    let first = rig.run(&ask(&path), &root);
    assert_eq!(rig.jobs.get(), 1, "a miss runs exactly one job: {}", first);
    assert_eq!(files_of(&first).len(), 3);
    let again = rig.run(&ask(&path), &root);
    assert_eq!(rig.jobs.get(), 1, "a hit runs no job");
    assert_eq!(first, again, "a hit answers the same three files");
    assert!(files_of(&again).iter().all(|f| Path::new(f).is_file()));
}

#[test]
fn a_new_mtime_colour_size_or_text_draws_again_once() {
    let d = TestDir::new("fontkey");
    let rig = Rig::new();
    let path = font(&d, "Inter-Bold.ttf");
    let root = d.join("cache");
    rig.run(&ask(&path), &root);
    let mut other = ask(&path);
    other.fg = "#ffffff".into();
    rig.run(&other, &root);
    assert_eq!(rig.jobs.get(), 2, "a theme change draws again");
    rig.run(&other, &root);
    assert_eq!(rig.jobs.get(), 2, "and once");
    let mut sized = ask(&path);
    sized.sizes = vec![44, 30, 24];
    rig.run(&sized, &root);
    assert_eq!(rig.jobs.get(), 3, "a different pixel ratio draws again");
    let mut texted = ask(&path);
    texted.text = "Sphinx of black quartz".into();
    rig.run(&texted, &root);
    assert_eq!(rig.jobs.get(), 4);
    let mut bg = ask(&path);
    bg.bg = "#000000".into();
    rig.run(&bg, &root);
    assert_eq!(rig.jobs.get(), 5);
    let stamp = std::fs::File::options()
        .write(true)
        .open(&path)
        .expect("open");
    stamp
        .set_modified(SystemTime::now() + Duration::from_secs(3600))
        .expect("touch");
    rig.run(&ask(&path), &root);
    assert_eq!(rig.jobs.get(), 6, "an edited font draws again");
}

#[test]
fn samples_cached_before_literal_font_paths_are_drawn_again() {
    let d = TestDir::new("fontcache-literal-paths");
    let rig = Rig::new();
    let path = font(&d, "%[fx:1].ttf");
    let ask = ask(&path);
    let canonical = path.canonicalize().unwrap();
    let meta = canonical.metadata().unwrap();
    let stamp = crate::backend::fontinfo::cache::stamp(&canonical, &meta, &ask, &ask.text);
    let old = crate::backend::md5::hex(stamp.as_bytes());
    let new = key(&canonical, &meta, &ask, &ask.text);
    assert_ne!(old, new);
    let root = d.dir("cache");
    for i in 0..SAMPLES {
        std::fs::write(entry(&root, &old, i), PNG).unwrap();
    }
    assert!(
        cached(&root, &old).is_some(),
        "the old samples are a whole set"
    );
    let reply = rig.run(&ask, &root);
    assert!(!reply.contains("\"err\""), "{reply}");
    assert_eq!(
        rig.jobs.get(),
        1,
        "old fallback samples must not hide the repaired font path"
    );
    assert!(
        files_of(&reply).iter().all(|f| f.contains(&new)),
        "the drawn samples carry the key the old stamp was hashed from: {reply}"
    );
    rig.run(&ask, &root);
    assert_eq!(rig.jobs.get(), 1, "the redrawn samples are a hit");
}

#[test]
fn the_job_the_runner_gets_names_the_canonical_font_and_the_ask() {
    let d = TestDir::new("fontjob");
    let rig = Rig::new();
    let path = font(&d, "Real.ttf");
    let link = d.join("Link.ttf");
    std::os::unix::fs::symlink(&path, &link).expect("link");
    rig.run(&ask(&link), &d.join("cache"));
    let seen = rig.seen.borrow();
    let inner = &seen[0];
    let at = inner.iter().position(|a| a == "-font").expect("-font");
    assert_eq!(
        Path::new(&inner[at + 1]),
        path.canonicalize().unwrap(),
        "the jail binds the file itself"
    );
    for want in [
        "#c0caf5",
        "#1a1b26",
        "33",
        "22",
        "18",
        "label:The quick brown fox jumps over the lazy dog",
    ] {
        assert!(
            inner.iter().any(|a| a == want),
            "argv lacks {}: {:?}",
            want,
            inner
        );
    }
    assert_eq!(inner.iter().filter(|a| a.ends_with(".png")).count(), 3);
}

#[test]
fn a_font_on_a_stuck_mount_is_refused_without_a_job() {
    let d = TestDir::new("fontstuck");
    let rig = Rig::new();
    let path = font(&d, "Stuck.ttf");
    let _stuck = crate::backend::iomount::test_hold_stuck(crate::backend::iomount::mount_key(
        &path,
        &crate::backend::mountinfo::read_body(),
    ));
    let line = rig.run(&ask(&path), &d.join("cache"));
    assert!(
        crate::json::field_str(&line, "err").is_some_and(|e| e.ends_with("is not responding.")),
        "{}",
        line
    );
    assert_eq!(rig.jobs.get(), 0, "a stuck mount runs no job");
}

#[test]
fn a_file_that_is_not_a_ttf_otf_or_ttc_is_refused_before_any_job() {
    let d = TestDir::new("fontmime");
    let rig = Rig::new();
    for name in ["a.woff", "a.txt", "a.pcf", "noext"] {
        let line = rig.run(&ask(&font(&d, name)), &d.join("cache"));
        assert!(
            line.contains(r#""err":"#),
            "{} must be refused: {}",
            name,
            line
        );
    }
    for name in ["a.TTF", "b.otf", "c.ttc"] {
        let line = rig.run(&ask(&font(&d, name)), &d.join("cache"));
        assert!(
            line.contains(r#""files":"#),
            "{} must be drawn: {}",
            name,
            line
        );
    }
    assert_eq!(rig.jobs.get(), 3);
}

#[test]
fn an_alias_name_resolves_to_its_canonical_type() {
    let rig = Rig {
        mime: Db::from_str("50:application/x-font-ttf:*.ttf\n"),
        ..Rig::new()
    };
    let d = TestDir::new("fontalias");
    let line = rig.run(&ask(&font(&d, "a.ttf")), &d.join("cache"));
    assert!(line.contains(r#""files":"#), "{}", line);
}

#[test]
fn an_otf_mime_collision_accepts_font_tables_but_not_an_office_document() {
    let d = TestDir::new("fontotfcollision");
    // OTTO, one maxp table at byte 28, version 1.0 and 25 glyphs.
    let bytes = [
        b"OTTO".as_slice(),
        &[0, 1, 0, 0, 0, 0, 0, 0],
        b"maxp",
        &[0, 0, 0, 0, 0, 0, 0, 28, 0, 0, 0, 6, 0, 1, 0, 0, 0, 25],
    ]
    .concat();
    let paths = [d.join("actual.otf"), d.join("uppercase.OTF")];
    for path in &paths {
        d.assert_contains(path);
        std::fs::write(path, &bytes).expect("font tables");
    }
    for (i, types) in [
        "50:application/vnd.oasis.opendocument.formula-template:*.otf\n50:font/otf:*.otf\n",
        "50:font/otf:*.otf\n50:application/vnd.oasis.opendocument.formula-template:*.otf\n",
    ]
    .iter()
    .enumerate()
    {
        let rig = Rig {
            mime: Db::from_str(types),
            ..Rig::new()
        };
        for path in &paths {
            let line = rig.run(&ask(path), &d.join(&format!("cache-{}", i)));
            assert_eq!(files_of(&line).len(), 3, "{}", line);
            assert_eq!(field_usize(&line, "glyphs"), Some(25), "{}", line);
        }
        assert_eq!(
            rig.jobs.get(),
            paths.len(),
            "each MIME order draws both fonts"
        );
    }
    let rig = Rig {
        mime: Db::from_str("50:application/vnd.oasis.opendocument.formula-template:*.otf\n"),
        ..Rig::new()
    };
    let office = d.file("formula.otf", "not a font");
    let root = d.join("office-cache");
    let line = rig.run(&ask(&office), &root);
    assert_eq!(
        field_str(&line, "err").as_deref(),
        Some("that file has no readable font tables")
    );
    assert_eq!(rig.jobs.get(), 0, "an Office document runs no renderer");
    assert!(!root.exists(), "an Office document creates no cache");
}

#[path = "render_safety_tests.rs"]
mod safety;
