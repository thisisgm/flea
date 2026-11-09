use super::*;
use crate::backend::archive::Formats;
use crate::backend::archiveops::{compress, convert_one, extract, Password};
use crate::backend::testdir::TestDir;
use std::process::Command;
use std::time::{Duration, Instant};

static NO_CANCEL: AtomicBool = AtomicBool::new(false);
// A writer that opens a FIFO blocks for good, so a compress still running after this has hung.
const HANG: Duration = Duration::from_secs(20);

fn fifo(path: &Path) {
    assert!(Command::new("mkfifo")
        .arg(path)
        .status()
        .expect("mkfifo")
        .success());
}

fn put(d: &TestDir, name: &str, body: &str) {
    std::fs::create_dir_all(d.join(name).parent().unwrap()).expect("parent");
    d.file(name, body);
}

fn tool(name: &str, probe: &str) -> bool {
    let found = Command::new(name)
        .arg(probe)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !found {
        eprintln!("SKIP: no {name} on this box");
    }
    found
}

fn members(archive: &Path) -> Vec<String> {
    let out = Command::new("bsdtar")
        .arg("-tf")
        .arg(archive)
        .output()
        .expect("bsdtar -t");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim_end_matches('/').to_string())
        .collect()
}

// A tree of a file beside a FIFO and a socket, one level down and two; deep/fifo is a regular file named like a skipped one.
fn special_tree(d: &TestDir) {
    put(d, "src/tree/f.txt", "f");
    put(d, "src/tree/deep/g.txt", "g");
    put(d, "src/tree/deep/fifo", "regular");
    fifo(&d.join("src/tree/fifo"));
    fifo(&d.join("src/tree/deep/pipe"));
    std::os::unix::net::UnixListener::bind(d.join("src/tree/sock")).expect("socket");
    std::os::unix::fs::symlink("fifo", d.join("src/tree/link")).expect("link");
}

#[test]
fn a_zip_plan_leaves_out_each_special_file_by_its_own_path_and_keeps_links() {
    let d = TestDir::new("zipplan");
    special_tree(&d);
    fifo(&d.join("src/top"));
    let names = ["tree".to_string(), "top".to_string()];
    let plan = plan(&d.join("src"), &names, &NO_CANCEL).expect("a plan");
    assert_eq!(
        plan.names,
        ["tree"],
        "a selected FIFO is dropped from the names"
    );
    let mut skipped = plan.skipped.clone();
    skipped.sort();
    assert_eq!(skipped, ["tree/deep/pipe", "tree/fifo", "tree/sock"]);
    let mut argv = vec!["7z".to_string()];
    exclude(&mut argv, &plan.skipped);
    assert!(argv.contains(&"-x!tree/fifo".to_string()), "{argv:?}");
}

#[test]
fn a_zip_of_nothing_but_special_files_is_refused_with_a_sentence() {
    let d = TestDir::new("zipspecial");
    fifo(&d.join("p"));
    let e = plan(d.path(), &["p".to_string()], &NO_CANCEL)
        .err()
        .expect("refused");
    assert_eq!(e.msg, ONLY_SPECIAL);
}

// S4-4: libarchive reads a backslash in a zip member as a separator, so a\b.txt would come back as a/b.txt.
#[test]
fn a_backslash_name_is_refused_for_a_zip_selected_or_nested() {
    let d = TestDir::new("zipbackslash");
    put(&d, "a\\b.txt", "b");
    put(&d, "dir/c\\d.txt", "d");
    let top = plan(d.path(), &["a\\b.txt".to_string()], &NO_CANCEL)
        .err()
        .expect("refused");
    assert_eq!(
        top.msg,
        "a zip cannot keep the backslash in a\\b.txt, choose tar or 7z"
    );
    let nested = plan(d.path(), &["dir".to_string()], &NO_CANCEL)
        .err()
        .expect("refused");
    assert_eq!(
        nested.msg,
        "a zip cannot keep the backslash in dir/c\\d.txt, choose tar or 7z"
    );
}

// S4-3: 7z -tzip opened the FIFO to read it and never finished, holding the destination drive's queue; -t7z stores one unread.
#[test]
fn a_zip_of_a_folder_holding_special_files_finishes_by_every_writer() {
    if crate::backend::sandboxprobe::skipped() || !tool("bsdtar", "--version") {
        return;
    }
    let seven = tool("7z", "i");
    let arms: [(Formats, &str, Option<&str>, bool); 5] = [
        (Formats::from_tools(true, true), "zip", None, true),
        (Formats::from_tools(true, true), "zip", Some("pw"), true),
        (Formats::from_tools(true, false), "zip", None, false),
        (Formats::from_tools(true, true), "7z", None, true),
        (Formats::from_tools(true, true), "7z", Some("pw"), true),
    ];
    for (formats, format, secret, needs_7z) in arms {
        if needs_7z && !seven {
            continue;
        }
        let d = TestDir::new("zipfifo");
        special_tree(&d);
        let dest = d.join(&format!("o.{format}"));
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let (parent, out, stop) = (d.join("src"), dest.clone(), cancel.clone());
        let job = std::thread::spawn(move || {
            let password = secret.map(|s| Password {
                secret: s,
                encrypt_names: false,
            });
            compress(
                &formats,
                &parent,
                &["tree".to_string()],
                format,
                &out,
                &stop,
                password,
            )
        });
        let start = Instant::now();
        while !job.is_finished() && start.elapsed() < HANG {
            std::thread::sleep(Duration::from_millis(50));
        }
        let hung = !job.is_finished();
        cancel.store(true, Ordering::Relaxed);
        let result = job.join().expect("the job thread");
        assert!(
            !hung,
            "the {format} ({secret:?}, 7z {needs_7z}) was still running after {HANG:?}"
        );
        result.unwrap_or_else(|e| panic!("the {format} ({secret:?}, 7z {needs_7z}) failed: {e:?}"));
        if format == "zip" && secret.is_none() {
            let mut got = members(&dest);
            got.sort();
            assert_eq!(
                got,
                [
                    "tree",
                    "tree/deep",
                    "tree/deep/fifo",
                    "tree/deep/g.txt",
                    "tree/f.txt",
                    "tree/link"
                ],
                "7z {needs_7z}"
            );
        }
    }
}

// S4-2: bsdtar read a member operand that begins with @ as an archive to copy in, even after --.
#[test]
fn an_at_name_is_stored_as_itself_by_every_bsdtar_format() {
    if crate::backend::sandboxprobe::skipped() || !tool("bsdtar", "--version") {
        return;
    }
    let d = TestDir::new("zipat");
    put(&d, "inner/smuggled.txt", "inner");
    put(&d, "src/@other.tar", "the selected file");
    // The archive the @ name points at, beside it, as the soak fixture had it.
    assert!(Command::new("bsdtar")
        .arg("-cf")
        .arg(d.join("src/other.tar"))
        .arg("-C")
        .arg(d.join("inner"))
        .arg("smuggled.txt")
        .status()
        .expect("bsdtar")
        .success());
    put(&d, "src/-dash", "dash");
    put(&d, "src/@dir/q.txt", "q");
    let names = [
        "@other.tar".to_string(),
        "-dash".to_string(),
        "@dir".to_string(),
    ];
    for format in ["tar", "tar.gz", "tar.zst", "zip"] {
        let formats = Formats::from_tools(true, false);
        let dest = d.join(&format!("o.{format}"));
        compress(
            &formats,
            &d.join("src"),
            &names,
            format,
            &dest,
            &NO_CANCEL,
            None,
        )
        .unwrap_or_else(|e| panic!("{format}: {e:?}"));
        let mut got = members(&dest);
        got.sort();
        assert_eq!(
            got,
            ["-dash", "@dir", "@dir/q.txt", "@other.tar"],
            "{format} stores the names as selected"
        );
        let back = d.join(&format!("x-{format}"));
        extract(&formats, &dest, &back, &NO_CANCEL, None)
            .unwrap_or_else(|e| panic!("{format} extract: {e:?}"));
        assert_eq!(
            std::fs::read_to_string(back.join("@other.tar")).unwrap(),
            "the selected file",
            "{format}"
        );
    }
}

// S4-4's other half: a format that keeps a backslash round trips it, and a zip refuses before any tool runs.
#[test]
fn a_backslash_name_round_trips_where_kept_and_a_zip_refuses_it() {
    if crate::backend::sandboxprobe::skipped() || !tool("bsdtar", "--version") {
        return;
    }
    let seven = tool("7z", "i");
    let d = TestDir::new("zipbsround");
    put(&d, "src/a\\b.txt", "backslash");
    let names = ["a\\b.txt".to_string()];
    for (formats, format) in [
        (Formats::from_tools(true, true), "tar.gz"),
        (Formats::from_tools(true, true), "7z"),
    ] {
        if format == "7z" && !seven {
            continue;
        }
        let dest = d.join(&format!("o.{format}"));
        compress(
            &formats,
            &d.join("src"),
            &names,
            format,
            &dest,
            &NO_CANCEL,
            None,
        )
        .expect(format);
        let back = d.join(&format!("x-{format}"));
        extract(&formats, &dest, &back, &NO_CANCEL, None).expect(format);
        let got: Vec<_> = std::fs::read_dir(&back)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(got, ["a\\b.txt"], "{format} keeps the name");
    }
    for formats in [
        Formats::from_tools(true, true),
        Formats::from_tools(true, false),
    ] {
        let dest = d.join("o.zip");
        let e = compress(
            &formats,
            &d.join("src"),
            &names,
            "zip",
            &dest,
            &NO_CANCEL,
            None,
        )
        .expect_err("refused");
        assert_eq!(
            e.msg,
            "a zip cannot keep the backslash in a\\b.txt, choose tar or 7z"
        );
        assert!(dest.symlink_metadata().is_err(), "and nothing is written");
    }
}

// magick expanded %d anywhere in the staged output path, so o%d.jpg came out as o0.jpg and scan%d sent it to scan0.
#[test]
fn a_convert_output_path_holding_percent_d_is_written_as_itself() {
    if crate::backend::sandboxprobe::skipped() || !tool("magick", "-version") {
        return;
    }
    let d = TestDir::new("convertpct");
    let input = d.join("in.png");
    assert!(Command::new("magick")
        .args(["-size", "2x2", "xc:red"])
        .arg(&input)
        .status()
        .expect("magick")
        .success());
    for dir in ["scan%d", "scan0"] {
        std::fs::create_dir_all(d.join(dir)).expect("folder");
    }
    for dest in [d.join("o%d.jpg"), d.join("scan%d/o%x.jpg")] {
        convert_one(&input, &dest, false, &NO_CANCEL).expect("the convert");
        assert!(dest.is_file(), "{dest:?} keeps its own path");
    }
    assert!(d.join("o0.jpg").symlink_metadata().is_err());
    assert_eq!(std::fs::read_dir(d.join("scan0")).unwrap().count(), 0);
}
