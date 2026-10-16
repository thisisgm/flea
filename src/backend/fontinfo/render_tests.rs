use super::*;
use crate::backend::testdir::TestDir;
use std::cell::{Cell, RefCell};
use std::time::SystemTime;

const TYPES: &str = "50:font/ttf:*.ttf\n50:font/otf:*.otf\n50:font/collection:*.ttc\n50:font/woff:*.woff\n50:text/plain:*.txt\n";
const ALIASES: &str = "application/x-font-ttf font/ttf\napplication/x-font-otf font/otf\n";
const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];

fn ask(path: &Path) -> Ask {
    Ask { id: 4, path: path.to_path_buf(), fg: "#c0caf5".into(), bg: "#1a1b26".into(), sizes: vec![33, 22, 18],
          text: "The quick brown fox jumps over the lazy dog".into() }
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
        Rig { jobs: Cell::new(0), seen: RefCell::new(Vec::new()), mime: Db::from_str(TYPES), aliases: Aliases::from_str(ALIASES) }
    }

    // The stub renderer: records its argv and draws the three PNGs where the argv names them.
    fn run(&self, ask: &Ask, root: &Path) -> String {
        answer(ask, &self.mime, &self.aliases, root, "magick", &mut |inner, _font, work| {
            self.jobs.set(self.jobs.get() + 1);
            self.seen.borrow_mut().push(inner.to_vec());
            for i in 0..SAMPLES {
                std::fs::write(work.dir.join(format!("{}.png", i)), PNG).expect("stub draws");
            }
            Ok(())
        })
    }
}

fn files_of(line: &str) -> Vec<String> {
    crate::json::field_str_array(line, "files")
}

#[test]
fn the_argv_carries_the_font_the_colours_the_sizes_and_the_text_in_one_run() {
    let d = TestDir::new("fontargv");
    let out = vec![d.join("0.png"), d.join("1.png"), d.join("2.png")];
    let got = argv("magick", Path::new("/f/Inter-Bold.ttf"), &ask(Path::new("/f/Inter-Bold.ttf")), "The quick brown fox", &out);
    let want: Vec<String> = ["magick", "-background", "#1a1b26", "-fill", "#c0caf5", "-font", "/f/Inter-Bold.ttf",
        "-pointsize", "33", "label:The quick brown fox", "-write", out[0].to_str().unwrap(), "+delete",
        "-pointsize", "22", "label:The quick brown fox", "-write", out[1].to_str().unwrap(), "+delete",
        "-pointsize", "18", "label:The quick brown fox", out[2].to_str().unwrap()].iter().map(|s| s.to_string()).collect();
    assert_eq!(got, want);
}

#[test]
fn the_text_and_its_percent_and_at_sign_are_closed_before_label_reads_them() {
    assert_eq!(label_text("100% sure").unwrap(), "100%% sure");
    for bad in ["", "@/etc/passwd", "a\\nb", "a\nb", &"x".repeat(MAX_TEXT_BYTES + 1)] {
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
    let stamp = std::fs::File::options().write(true).open(&path).expect("open");
    stamp.set_modified(SystemTime::now() + Duration::from_secs(3600)).expect("touch");
    rig.run(&ask(&path), &root);
    assert_eq!(rig.jobs.get(), 6, "an edited font draws again");
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
    assert_eq!(Path::new(&inner[at + 1]), path.canonicalize().unwrap(), "the jail binds the file itself");
    for want in ["#c0caf5", "#1a1b26", "33", "22", "18", "label:The quick brown fox jumps over the lazy dog"] {
        assert!(inner.iter().any(|a| a == want), "argv lacks {}: {:?}", want, inner);
    }
    assert_eq!(inner.iter().filter(|a| a.ends_with(".png")).count(), 3);
}

#[test]
fn a_font_on_a_stuck_mount_is_refused_without_a_job() {
    let d = TestDir::new("fontstuck");
    let rig = Rig::new();
    let path = font(&d, "Stuck.ttf");
    let _stuck = crate::backend::iomount::test_hold_stuck(crate::backend::iomount::mount_key(&path, &crate::backend::mountinfo::read_body()));
    let line = rig.run(&ask(&path), &d.join("cache"));
    assert!(crate::json::field_str(&line, "err").is_some_and(|e| e.ends_with("is not responding.")), "{}", line);
    assert_eq!(rig.jobs.get(), 0, "a stuck mount runs no job");
}

#[test]
fn a_file_that_is_not_a_ttf_otf_or_ttc_is_refused_before_any_job() {
    let d = TestDir::new("fontmime");
    let rig = Rig::new();
    for name in ["a.woff", "a.txt", "a.pcf", "noext"] {
        let line = rig.run(&ask(&font(&d, name)), &d.join("cache"));
        assert!(line.contains(r#""err":"#), "{} must be refused: {}", name, line);
    }
    for name in ["a.TTF", "b.otf", "c.ttc"] {
        let line = rig.run(&ask(&font(&d, name)), &d.join("cache"));
        assert!(line.contains(r#""files":"#), "{} must be drawn: {}", name, line);
    }
    assert_eq!(rig.jobs.get(), 3);
}

#[test]
fn an_alias_name_resolves_to_its_canonical_type() {
    let rig = Rig { mime: Db::from_str("50:application/x-font-ttf:*.ttf\n"), ..Rig::new() };
    let d = TestDir::new("fontalias");
    let line = rig.run(&ask(&font(&d, "a.ttf")), &d.join("cache"));
    assert!(line.contains(r#""files":"#), "{}", line);
}

#[test]
fn an_otf_mime_collision_accepts_font_tables_but_not_an_office_document() {
    let d = TestDir::new("fontotfcollision");
    let path = d.join("actual.otf");
    // OTTO, one maxp table at byte 28, version 1.0 and 25 glyphs.
    let bytes = [b"OTTO".as_slice(), &[0, 1, 0, 0, 0, 0, 0, 0], b"maxp",
        &[0, 0, 0, 0, 0, 0, 0, 28, 0, 0, 0, 6, 0, 1, 0, 0, 0, 25]].concat();
    std::fs::write(&path, bytes).unwrap();
    for types in [
        "50:application/vnd.oasis.opendocument.formula-template:*.otf\n50:font/otf:*.otf\n",
        "50:font/otf:*.otf\n50:application/vnd.oasis.opendocument.formula-template:*.otf\n",
    ] {
        let rig = Rig { mime: Db::from_str(types), ..Rig::new() };
        assert_eq!(files_of(&rig.run(&ask(&path), &d.join("cache"))).len(), 3);
    }
    let rig = Rig { mime: Db::from_str("50:application/vnd.oasis.opendocument.formula-template:*.otf\n"), ..Rig::new() };
    let office = d.file("formula.otf", "not a font");
    assert!(rig.run(&ask(&office), &d.join("cache")).contains("no readable font tables"));
    assert_eq!(rig.jobs.get(), 0);
}

#[test]
fn a_bad_ask_runs_no_job() {
    let d = TestDir::new("fontbad");
    let rig = Rig::new();
    let path = font(&d, "a.ttf");
    let mut relative = ask(&path);
    relative.path = PathBuf::from("a.ttf");
    let mut colour = ask(&path);
    colour.fg = "red".into();
    let mut sizes = ask(&path);
    sizes.sizes = vec![33, 22];
    let mut huge = ask(&path);
    huge.sizes = vec![33, 22, 100000];
    for bad in [relative, colour, sizes, huge, ask(&d.join("missing.ttf")), ask(d.path())] {
        let line = rig.run(&bad, &d.join("cache"));
        assert!(line.contains(r#""err":"#), "{}", line);
    }
    assert_eq!(rig.jobs.get(), 0);
}

#[test]
fn a_failed_job_or_a_wrong_output_leaves_nothing_in_the_cache() {
    let d = TestDir::new("fontfail");
    let rig = Rig::new();
    let path = font(&d, "a.ttf");
    let root = d.join("cache");
    let failed = answer(&ask(&path), &rig.mime, &rig.aliases, &root, "magick", &mut |_, _, _| Err(op_err("font", "magick", "the font tool exited with status 1")));
    assert_eq!(failed, r#"{"t":"fontlooked","id":4,"err":"the font tool exited with status 1"}"#);
    let wrote_text = answer(&ask(&path), &rig.mime, &rig.aliases, &root, "magick", &mut |_, _, work| {
        for i in 0..SAMPLES {
            std::fs::write(work.dir.join(format!("{}.png", i)), b"not a png at all").expect("write");
        }
        Ok(())
    });
    assert!(wrote_text.contains("did not draw"), "{}", wrote_text);
    let wrote_two = answer(&ask(&path), &rig.mime, &rig.aliases, &root, "magick", &mut |_, _, work| {
        std::fs::write(work.dir.join("0.png"), PNG).expect("write");
        std::fs::write(work.dir.join("1.png"), PNG).expect("write");
        Ok(())
    });
    assert!(wrote_two.contains("did not draw"), "{}", wrote_two);
    let left: Vec<_> = std::fs::read_dir(&root).expect("root").flatten().map(|e| e.file_name()).collect();
    assert!(left.is_empty(), "a failed job left {:?}", left);
}

#[test]
fn a_symlink_planted_at_a_cache_path_is_never_read_or_followed() {
    let d = TestDir::new("fontplant");
    let rig = Rig::new();
    let path = font(&d, "a.ttf");
    let root = d.join("cache");
    let first = rig.run(&ask(&path), &root);
    // A real PNG, so only the symlink itself can make it a miss.
    let victim = d.join("victim.png");
    std::fs::write(&victim, [PNG, b"keep me"].concat()).expect("victim");
    let planted = PathBuf::from(&files_of(&first)[1]);
    std::fs::remove_file(&planted).expect("remove");
    std::os::unix::fs::symlink(&victim, &planted).expect("plant");
    let again = rig.run(&ask(&path), &root);
    assert_eq!(rig.jobs.get(), 2, "a symlink is a miss");
    assert_eq!(std::fs::read(&victim).unwrap(), [PNG, b"keep me"].concat(), "the planted target is untouched");
    assert!(!planted.symlink_metadata().unwrap().file_type().is_symlink(), "the entry is a plain file again");
    assert_eq!(files_of(&again).len(), 3);
}

#[test]
fn the_cache_directory_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let d = TestDir::new("fontmode");
    Rig::new().run(&ask(&font(&d, "a.ttf")), &d.join("cache"));
    assert_eq!(std::fs::metadata(d.join("cache")).unwrap().permissions().mode() & 0o777, 0o700);
}

#[test]
fn the_wire_lines_have_the_documented_shape() {
    let want = r##"{"c":"fontlook","id":4,"path":"/f/Inter-Bold.ttf","fg":"#c0caf5","bg":"#1a1b26","sizes":[33,22,18],"text":"The quick"}"##;
    let parsed = Ask::parse(want);
    assert_eq!((parsed.id, parsed.fg.as_str(), parsed.sizes.clone(), parsed.text.as_str()), (4, "#c0caf5", vec![33, 22, 18], "The quick"));
    let facts = Facts { family: Some("Inter".into()), style: Some("Bold".into()), glyphs: Some(2548) };
    let files = vec![PathBuf::from("/c/a-0.png"), PathBuf::from("/c/a-1.png"), PathBuf::from("/c/a-2.png")];
    assert_eq!(looked_line(4, &facts, &files),
        r#"{"t":"fontlooked","id":4,"family":"Inter","style":"Bold","glyphs":2548,"files":["/c/a-0.png","/c/a-1.png","/c/a-2.png"]}"#);
    assert_eq!(looked_line(5, &Facts::default(), &files), r#"{"t":"fontlooked","id":5,"files":["/c/a-0.png","/c/a-1.png","/c/a-2.png"]}"#);
    assert_eq!(failed_line(6, "no \"x\""), r#"{"t":"fontlooked","id":6,"err":"no \"x\""}"#);
}

// The production runner's jail with a stub in place of magick: the work folder is writable, the font is read only.
#[test]
fn the_jail_lets_the_stub_write_the_work_folder_and_not_the_font() {
    if crate::backend::sandboxprobe::skipped() { return; }
    let d = TestDir::new("fontjail");
    let path = font(&d, "a.ttf");
    let root = d.dir("cache");
    let mut work = Work::new(&root, WORK_TAG).expect("work");
    let script = "import sys\nfor p in sys.argv[2:]:\n    open(p, 'wb').write(bytes([0x89, 0x50, 0x4e, 0x47, 13, 10, 26, 10]))\ntry:\n    open(sys.argv[1], 'ab')\n    sys.exit(3)\nexcept OSError:\n    pass\n";
    let drawn: Vec<String> = (0..SAMPLES).map(|i| work.dir.join(format!("{}.png", i)).to_string_lossy().to_string()).collect();
    let mut inner = vec!["/usr/bin/python3".to_string(), "-c".to_string(), script.to_string(), path.to_string_lossy().to_string()];
    inner.extend(drawn);
    let cancel = AtomicBool::new(false);
    let result = run_boxed_cancellable_capped("font", inner, &path, &mut work, &cancel);
    assert!(result.is_ok(), "the stub must write its folder and fail to write the font: {:?}", result.err().map(|e| e.msg));
    assert!((0..SAMPLES).all(|i| is_png(&work.dir.join(format!("{}.png", i)))));
}
