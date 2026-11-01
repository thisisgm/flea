use super::*;
use crate::backend::aliases::Aliases;
use crate::backend::testdir::TestDir;
use crate::backend::thumbcache::{png_text, Cache, Hit};
use crate::backend::thumbspec::Thumbnailers;
use crate::backend::thumbwrite::write_marker;
use std::sync::Arc;
use std::time::Instant;
#[path = "appicon_fixtures.rs"]
mod fixtures;

const MTIME: i64 = 1787790423;
const APPIMAGE: &str = "application/vnd.appimage";
// Stands in for 7z: `l` prints the image file itself, which holds the canned listing; `e` writes the canned bytes under the member's base name, or a symlink for a member the test names as one.
const STUB: &str = r#"mode=$1; shift
for a in "$@"; do last=$a; case $a in -o*) out=${a#-o};; esac; done
case $mode in
  l) cat "$last";;
  e) mkdir -p "$out" && case $last in __LINKS__ *) printf '__BYTES__' > "$out/${last##*/}";; esac;;
esac"#;

// Runs the argv of one tool step without the namespaces, so the argv, the redirect and the parse are proved wherever the bwrap and prlimit binaries are on PATH, which run_with still checks first; the jail itself is proved only by the tests that skip when it cannot start. A nonzero exit is the tool failing, not the file.
fn unjailed(inner: Vec<String>, _image: &Path, _work: &mut Work, _limit: Duration) -> Result<(), Stop> {
    let status = std::process::Command::new(&inner[0]).args(&inner[1..]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    if status.is_ok_and(|s| s.success()) { Ok(()) } else { Err(Stop::Fault) }
}

// The decoder argv as it is, for a box whose bwrap cannot start.
pub(super) fn plain(inner: &[String], _input: &Path, _out: &Path) -> Vec<String> {
    inner.to_vec()
}

// A jail that would not start.
fn no_jail(_inner: Vec<String>, _image: &Path, _work: &mut Work, _limit: Duration) -> Result<(), Stop> {
    Err(Stop::Fault)
}

fn octal(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{:03o}", b)).collect()
}

fn stub(member: &[u8]) -> Vec<String> {
    stub_links(member, &[])
}

// A member named in links is extracted as a symlink to its target, the way 7z extracts a squashfs link; every other member holds the bytes.
fn stub_links(member: &[u8], links: &[(&str, &str)]) -> Vec<String> {
    let arms: String = links.iter().map(|(name, target)| format!("{}) ln -s {} \"$out/${{last##*/}}\";; ", name, target)).collect();
    let script = STUB.replace("__LINKS__ ", &arms).replace("__BYTES__", &octal(member));
    ["/usr/bin/sh", "-c", &script, "stub"].map(String::from).to_vec()
}

// Sample records: ("A.png", "Symbolic Link = B.png") is a link, ("B.png", "") a file.
pub(super) fn listing(records: &[(&str, &str)]) -> String {
    let mut text = String::from("Path = /x/app.AppImage\nType = SquashFS\n\n----------\n");
    for (path, extra) in records {
        text.push_str(&format!("Path = {}\nSize = 67\nFolder = -\n{}\n\n", path, extra));
    }
    text
}

fn png(dir: &TestDir) -> Vec<u8> {
    let path = dir.join("icon.png");
    write_marker(&path, "file:///icon", 0, None).unwrap();
    std::fs::read(path).unwrap()
}

pub(super) struct Fixture {
    _dir: TestDir,
    pub(super) tables: Tables,
    pub(super) job: Job,
}

pub(super) fn fixture(tag: &str, text: &str) -> Fixture {
    let dir = TestDir::new(tag);
    dir.file("app.AppImage", text);
    let aliases = Arc::new(Aliases::load());
    let specs = Arc::new(Thumbnailers::load(&aliases).with_appimage_route(&aliases, true));
    let cache = Cache::at(dir.join("cache"));
    let job = Job { replies: None, path: dir.join("app.AppImage"), mtime: MTIME, trash_identity: None, mime: APPIMAGE.to_string(), trace: None };
    Fixture { _dir: dir, tables: Tables { aliases, specs, cache, worker: None }, job }
}

fn published(f: &Fixture) -> usize {
    std::fs::read_dir(f.tables.cache.large_dir()).map(|d| d.count()).unwrap_or(0)
}

pub(super) fn failed_and_recorded(f: &Fixture, out: Outcome) {
    assert!(matches!(out, Outcome::Failed));
    assert!(matches!(f.tables.cache.lookup(&f.job.path, MTIME, f.tables.specs.fingerprint()), Hit::Failed), "no fail marker was recorded");
    assert_eq!(published(f), 0, "a failed job left a file in large/");
}

#[test]
fn a_png_dir_icon_publishes_once_and_the_cache_answers_the_second_request() {
    let d = TestDir::new("appicon-bytes");
    let bytes = png(&d);
    let f = fixture("appicon-png", &listing(&[(".DirIcon", "")]));
    let out = run_with(&stub(&bytes), unjailed, plain, &f.tables, &f.job);
    let Outcome::Ready(path) = out else { panic!("a PNG .DirIcon did not publish") };
    assert_eq!(png_text(&std::fs::read(&path).unwrap(), "Thumb::URI"), Some(uri_for(&f.job.path)));
    assert_eq!(published(&f), 1, "a temp or work folder survived the publish");
    // A request that finds this answer never queues a job, so a second one at the same mtime runs nothing.
    assert!(matches!(f.tables.cache.lookup(&f.job.path, MTIME, f.tables.specs.fingerprint()), Hit::Ready(_)));
}

#[test]
fn the_jail_runs_the_same_chain() {
    if crate::backend::sandboxprobe::skipped() { return; }
    let bytes = png(&TestDir::new("appicon-bytes"));
    let f = fixture("appicon-jail", &listing(&[(".DirIcon", "Symbolic Link = app.png"), ("app.png", "")]));
    assert!(matches!(run_with(&stub(&bytes), within, jailed, &f.tables, &f.job), Outcome::Ready(_)));
    let f = fixture("appicon-jail-none", &listing(&[("AppRun", "")]));
    failed_and_recorded(&f, run_with(&stub(&bytes), within, jailed, &f.tables, &f.job));
}

#[test]
fn a_symlink_chain_inside_the_image_resolves() {
    let bytes = png(&TestDir::new("appicon-bytes"));
    let text = listing(&[(".DirIcon", "Symbolic Link = share/app.png"), ("share/app.png", "Symbolic Link = ../real/app.png"), ("real/app.png", "")]);
    let f = fixture("appicon-chain", &text);
    assert!(matches!(run_with(&stub(&bytes), unjailed, plain, &f.tables, &f.job), Outcome::Ready(_)));
}

#[test]
fn a_chain_that_leaves_the_image_is_refused_and_recorded() {
    let bytes = png(&TestDir::new("appicon-bytes"));
    for (tag, target) in [("appicon-up", "../../etc/passwd"), ("appicon-abs", "/usr/share/icons/app.png")] {
        let f = fixture(tag, &listing(&[(".DirIcon", &format!("Symbolic Link = {}", target)), ("etc/passwd", ""), ("usr/share/icons/app.png", "")]));
        failed_and_recorded(&f, run_with(&stub(&bytes), unjailed, plain, &f.tables, &f.job));
    }
}

#[test]
fn a_loop_is_refused_and_recorded() {
    let bytes = png(&TestDir::new("appicon-bytes"));
    let f = fixture("appicon-loop", &listing(&[(".DirIcon", "Symbolic Link = a.png"), ("a.png", "Symbolic Link = .DirIcon")]));
    failed_and_recorded(&f, run_with(&stub(&bytes), unjailed, plain, &f.tables, &f.job));
}

#[test]
fn a_missing_dir_icon_records_a_failure_and_a_non_png_member_does_too() {
    let bytes = png(&TestDir::new("appicon-bytes"));
    let f = fixture("appicon-none", &listing(&[("AppRun", "")]));
    failed_and_recorded(&f, run_with(&stub(&bytes), unjailed, plain, &f.tables, &f.job));
    let f = fixture("appicon-text", &listing(&[(".DirIcon", "")]));
    failed_and_recorded(&f, run_with(&stub(b"not an image"), unjailed, plain, &f.tables, &f.job));
}

#[test]
fn a_missing_7z_records_nothing_when_the_sandbox_is_there() {
    let missing = vec!["/definitely/not/7z".to_string()];
    // The sandbox check comes first in run_with, so without it this would pass from that branch and say nothing about the tool.
    if !sandbox::available() {
        eprintln!("SKIP a_missing_7z_records_nothing_when_the_sandbox_is_there: bwrap or prlimit is not on PATH");
        return;
    }
    let f = fixture("appicon-no-tool", &listing(&[(".DirIcon", "")]));
    assert!(matches!(run_with(&missing, unjailed, plain, &f.tables, &f.job), Outcome::Failed));
    assert!(matches!(f.tables.cache.lookup(&f.job.path, MTIME, f.tables.specs.fingerprint()), Hit::Miss), "a missing package was recorded as a broken file");
    assert_eq!(published(&f), 0, "a job that never started left a file in large/");
}

#[test]
fn a_job_needs_both_the_sandbox_and_the_tool() {
    let there = vec!["/usr/bin/sh".to_string()];
    let missing = vec!["/definitely/not/7z".to_string()];
    assert!(usable(&there, true));
    assert!(!usable(&there, false), "a job ran without the sandbox");
    assert!(!usable(&missing, true), "a job ran without the tool");
    assert!(!usable(&[], true));
}

#[test]
fn a_row_has_a_route_only_when_7z_is_there() {
    let aliases = Aliases::load();
    let without = Thumbnailers::from_entries(&[], &aliases).with_appimage_route(&aliases, false);
    assert!(without.for_mime(APPIMAGE, &aliases).is_none(), "the row would be asked for an icon it cannot get");
    let with = Thumbnailers::from_entries(&[], &aliases).with_appimage_route(&aliases, true);
    for mime in MIMES {
        assert!(with.for_mime(mime, &aliases).is_some_and(is_route), "{} has no route", mime);
    }
}

#[test]
fn a_jail_that_will_not_start_records_nothing_and_a_refusal_about_the_file_does() {
    let bytes = png(&TestDir::new("appicon-bytes"));
    let f = fixture("appicon-fault", &listing(&[(".DirIcon", "")]));
    assert!(matches!(run_with(&stub(&bytes), no_jail, plain, &f.tables, &f.job), Outcome::Failed));
    assert!(matches!(f.tables.cache.lookup(&f.job.path, MTIME, f.tables.specs.fingerprint()), Hit::Miss), "a jail that would not start was recorded as a broken file");
    assert_eq!(published(&f), 0, "a job that never ran left a file in large/");
    let f = fixture("appicon-refused", &listing(&[("AppRun", "")]));
    failed_and_recorded(&f, run_with(&stub(&bytes), unjailed, plain, &f.tables, &f.job));
}

#[test]
fn only_7z_calling_the_file_not_an_archive_is_the_files_fault() {
    let err = |msg: &str| crate::backend::opsreq::op_err("appimage icon", "", msg);
    assert_eq!(stop_for(&err("ERROR: /x/a.AppImage : Cannot open the file as archive"), false), Stop::Refused);
    assert_eq!(stop_for(&err("the appimage icon tool exited with status 2"), false), Stop::Fault);
    assert_eq!(stop_for(&err("the appimage icon tool was killed by signal SIGSEGV (11)"), false), Stop::Fault);
    let spawn = crate::error::from_io("appimage icon", "bwrap", &std::io::Error::from_raw_os_error(11));
    assert_eq!(stop_for(&spawn, false), Stop::Fault);
    assert_eq!(stop_for(&err("cancelled"), true), Stop::TimedOut);
}

#[test]
fn the_jail_tells_a_non_archive_from_a_crash() {
    if crate::backend::sandboxprobe::skipped() { return; }
    let d = TestDir::new("appicon-within");
    let image = d.file("a.AppImage", "x");
    let mut work = Work::new(d.path(), WORK_TAG).unwrap();
    let sh = |script: &str| ["/usr/bin/sh", "-c", script].map(String::from).to_vec();
    assert_eq!(within(sh("echo 'ERROR: a : Cannot open the file as archive' >&2; exit 2"), &image, &mut work, Duration::from_secs(10)), Err(Stop::Refused));
    assert_eq!(within(sh("exit 2"), &image, &mut work, Duration::from_secs(10)), Err(Stop::Fault));
    assert_eq!(within(sh("sleep 5"), &image, &mut work, Duration::from_millis(300)), Err(Stop::TimedOut));
}

#[test]
fn a_work_folder_that_cannot_be_made_takes_its_temp_with_it() {
    fn full(_dir: &Path, _tag: &str) -> Result<Work, FleaError> {
        Err(crate::backend::opsreq::op_err("archive", "", "no space left on device"))
    }
    let d = TestDir::new("appicon-begin");
    assert!(begin(d.path(), full).is_none());
    let left: Vec<_> = std::fs::read_dir(d.path()).unwrap().flatten().map(|e| e.file_name()).filter(|n| n != ".flea-test-sandbox").collect();
    assert!(left.is_empty(), "an exclusive temp survived a failed work folder: {:?}", left);
    assert!(begin(d.path(), Work::new).is_some());
}

#[test]
fn a_thumbnailer_file_that_declares_an_appimage_keeps_it() {
    let aliases = Aliases::load();
    let entries = [("x.thumbnailer".to_string(), "[Thumbnailer Entry]\nExec=/usr/bin/true %i\nMimeType=application/vnd.appimage;\n".to_string())];
    let specs = Thumbnailers::from_entries(&entries, &aliases).with_appimage_route(&aliases, true);
    assert!(!is_route(specs.for_mime(APPIMAGE, &aliases).unwrap()));
}

fn no_links(_: &str) -> Result<String, Stop> {
    panic!("a link the listing names was read from the image")
}

// Resolves with the links answered from a table, as the extract of a link member answers them.
fn resolve_with(text: &str, links: &[(&str, &str)]) -> Result<String, Stop> {
    resolve(text, &mut |at| links.iter().find(|(name, _)| *name == at).map(|(_, to)| to.to_string()).ok_or(Stop::Refused))
}

#[test]
fn the_chain_is_followed_at_most_four_links_and_only_inside_the_image() {
    let link = |to: &str| format!("Symbolic Link = {}", to);
    let four = listing(&[(".DirIcon", &link("a")), ("a", &link("b")), ("b", &link("c")), ("c", &link("d")), ("d", "")]);
    assert_eq!(resolve(&four, &mut no_links), Ok("d".to_string()));
    let five = listing(&[(".DirIcon", &link("a")), ("a", &link("b")), ("b", &link("c")), ("c", &link("d")), ("d", &link("e")), ("e", "")]);
    assert!(resolve(&five, &mut no_links).is_err(), "a fifth link was followed");
    let out = listing(&[(".DirIcon", &link("usr/../../x")), ("x", "")]);
    assert!(resolve(&out, &mut no_links).is_err(), "a link climbed past the image root");
    let inside = listing(&[(".DirIcon", &link("usr/../icon.png")), ("icon.png", "")]);
    assert_eq!(resolve(&inside, &mut no_links), Ok("icon.png".to_string()));
    let nested = listing(&[(".DirIcon", &link("usr/share/a.png")), ("usr/share/a.png", &link("../b.png")), ("usr/b.png", "")]);
    assert_eq!(resolve(&nested, &mut no_links), Ok("usr/b.png".to_string()));
    assert!(resolve(&listing(&[("AppRun", "")]), &mut no_links).is_err());
    assert!(resolve("garbage", &mut no_links).is_err());
}

// 7z 25.01 on a squashfs AppImage prints "Mode = lrwxrwxrwx" and "Size = 7" for a link, and no target.
#[test]
fn a_link_the_listing_does_not_name_is_read_from_the_image() {
    let link = "Mode = lrwxrwxrwx";
    let text = listing(&[(".DirIcon", link), ("AppRun", ""), ("share/a.png", link), ("real/b.png", "")]);
    assert_eq!(resolve_with(&text, &[(".DirIcon", "share/a.png"), ("share/a.png", "../real/b.png")]), Ok("real/b.png".to_string()));
    assert!(resolve_with(&text, &[(".DirIcon", "/etc/passwd")]).is_err(), "an absolute link was followed");
    assert!(resolve_with(&text, &[(".DirIcon", ".DirIcon")]).is_err(), "a loop was followed");
    assert_eq!(resolve_with(&text, &[]), Err(Stop::Refused));
}

#[test]
fn a_symlink_dir_icon_that_7z_lists_without_a_target_publishes() {
    let bytes = png(&TestDir::new("appicon-bytes"));
    let text = listing(&[(".DirIcon", "Mode = lrwxrwxrwx"), ("app.png", "Mode = -rw-r--r--")]);
    let f = fixture("appicon-modelink", &text);
    let tool = stub_links(&bytes, &[(".DirIcon", "app.png")]);
    assert!(matches!(run_with(&tool, unjailed, plain, &f.tables, &f.job), Outcome::Ready(_)));
    assert_eq!(published(&f), 1);
}

fn base64(bytes: &[u8]) -> String {
    const SET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { SET[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

// A thumbnailer that writes the canned 1x1 PNG for the given types, so a test sees which spec a member went through and what it published.
fn thumbnailer(mimes: &str, answer: &[u8]) -> Thumbnailers {
    let aliases = Aliases::load();
    let text = format!("[Thumbnailer Entry]\nExec=/usr/bin/sh -c \"echo {} | /usr/bin/base64 -d > $0\" %o\nMimeType={};\n", base64(answer), mimes);
    Thumbnailers::from_entries(&[("t.thumbnailer".to_string(), text)], &aliases).with_appimage_route(&aliases, true)
}

fn with_specs(f: Fixture, specs: Thumbnailers) -> Fixture {
    Fixture { tables: Tables { specs: Arc::new(specs), ..f.tables }, ..f }
}

// The width and height an IHDR claims, big-endian at bytes 16 and 20.
fn sides(png: &[u8]) -> (u32, u32) {
    let at = |i: usize| u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]);
    (at(16), at(20))
}

// The canned PNG with an IHDR that claims a larger picture; only the signature and IHDR are read before the spec runs.
fn claiming(png: &[u8], side: u32) -> Vec<u8> {
    let mut big = png.to_vec();
    big[16..20].copy_from_slice(&side.to_be_bytes());
    big[20..24].copy_from_slice(&side.to_be_bytes());
    big
}

#[test]
fn a_dir_icon_over_the_thumbnail_size_goes_through_the_image_spec() {
    let small = png(&TestDir::new("appicon-bytes"));
    let f = with_specs(fixture("appicon-big", &listing(&[(".DirIcon", "")])), thumbnailer("image/png", &small));
    let Outcome::Ready(path) = run_with(&stub(&claiming(&small, 512)), unjailed, plain, &f.tables, &f.job) else { panic!("a 512 px .DirIcon did not publish") };
    let (w, h) = sides(&std::fs::read(path).unwrap());
    assert!(w <= THUMB_SIZE && h <= THUMB_SIZE, "a {}x{} icon was published as the large thumbnail", w, h);
    // At the size, the icon is published as it is and no thumbnailer is needed.
    let f = fixture("appicon-fits", &listing(&[(".DirIcon", "")]));
    assert!(matches!(run_with(&stub(&claiming(&small, THUMB_SIZE)), unjailed, plain, &f.tables, &f.job), Outcome::Ready(_)));
    // Without a thumbnailer for the type, a picture too big to publish records nothing.
    let f = fixture("appicon-nospec", &listing(&[(".DirIcon", "")]));
    let none = Thumbnailers::from_entries(&[], &Aliases::load());
    let f = with_specs(f, none);
    assert!(matches!(run_with(&stub(&claiming(&small, 512)), unjailed, plain, &f.tables, &f.job), Outcome::Failed));
    assert!(matches!(f.tables.cache.lookup(&f.job.path, MTIME, f.tables.specs.fingerprint()), Hit::Miss));
}

#[test]
fn an_svg_dir_icon_goes_through_the_svg_spec() {
    let small = png(&TestDir::new("appicon-bytes"));
    let f = with_specs(fixture("appicon-svg", &listing(&[(".DirIcon", "")])), thumbnailer("image/svg+xml", &small));
    let svg = b"<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'/>";
    let Outcome::Ready(path) = run_with(&stub(svg), unjailed, plain, &f.tables, &f.job) else { panic!("an SVG .DirIcon did not publish") };
    assert_eq!(png_text(&std::fs::read(&path).unwrap(), "Thumb::URI"), Some(uri_for(&f.job.path)));
    assert_eq!(published(&f), 1, "a temp or work folder survived the publish");
}

#[test]
fn the_svg_decoder_runs_only_for_the_time_left_of_the_job() {
    let d = TestDir::new("appicon-left");
    let sleeper = Thumbnailers::from_entries(&[("t.thumbnailer".to_string(), "[Thumbnailer Entry]\nExec=/usr/bin/sleep 8\nMimeType=image/svg+xml;\n".to_string())], &Aliases::load());
    let f = with_specs(fixture("appicon-left-f", &listing(&[(".DirIcon", "")])), sleeper);
    let member = d.file("icon.svg", "<svg xmlns='http://www.w3.org/2000/svg'/>");
    let temp = d.join("out.png");
    let started = Instant::now();
    assert!(matches!(make(&f.tables, &member, &temp, Duration::from_millis(400), plain), Ok(Ran::TimedOut)));
    assert!(started.elapsed() < Duration::from_secs(5), "the decoder ran past the time left");
    assert!(matches!(make(&f.tables, &member, &temp, Duration::ZERO, plain), Err(Stop::TimedOut)));
}

// The first of the names on PATH, so a box without the package skips instead of failing.
fn on_path(name: &str) -> Option<String> {
    std::env::var("PATH").ok()?.split(':').map(|dir| format!("{}/{}", dir, name)).find(|p| is_runnable(p))
}

// The tools the test shell-outs to, run for real: an ELF prefix followed by a squashfs, as a type 2 AppImage is.
#[test]
fn real_7z_reads_a_real_appimage_with_a_symlink_dir_icon() {
    let Some(seven) = on_path("7z") else {
        eprintln!("SKIP real_7z_reads_a_real_appimage_with_a_symlink_dir_icon: 7z is not installed");
        return;
    };
    let d = TestDir::new("appicon-real");
    let mut image = std::fs::read("/usr/bin/true").unwrap();
    image.extend(fixtures::checked(&d, &seven, true));
    let f = fixture("appicon-real-f", "");
    std::fs::write(&f.job.path, &image).unwrap();
    let Outcome::Ready(path) = run_with(std::slice::from_ref(&seven), unjailed, plain, &f.tables, &f.job) else { panic!("real 7z did not yield the icon of a squashfs AppImage") };
    assert_eq!(png_text(&std::fs::read(path).unwrap(), "Thumb::URI"), Some(uri_for(&f.job.path)));
    // The same image with no .DirIcon is the image's answer, so it is recorded.
    let mut bare = std::fs::read("/usr/bin/true").unwrap();
    bare.extend(fixtures::checked(&d, &seven, false));
    let f = fixture("appicon-real-none", "");
    std::fs::write(&f.job.path, &bare).unwrap();
    failed_and_recorded(&f, run_with(&[seven], unjailed, plain, &f.tables, &f.job));
}
