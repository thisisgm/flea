// A subprocess drives the production JSON loop with private PATH stubs and private XDG roots.
use super::*;
use crate::backend::testdir::TestDir;
use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

const HELPER: &str = "backend::printreq::tests::print_protocol_helper";

#[path = "print_real_tests.rs"]
mod real;
#[path = "print_advloop_tests.rs"]
mod advloop;
const STUB: &str = r#"#!/bin/sh
set -eu
root=${FLEA_PRINT_TEST_ROOT:?}
case "$root" in /*) test -f "$root/.flea-test-sandbox" ;; *) exit 98 ;; esac
tool=${0##*/}
printf '%s\0' "$tool" "$@" >> "$root/calls"
printf '\n' >> "$root/calls"
case "$tool" in
 prlimit) while [ "${1#--}" != "$1" ]; do shift; done; exec "$@" ;;
 bwrap)
  while [ "$#" -gt 0 ]; do
   case "$1" in
    --ro-bind|--ro-bind-try|--bind|--symlink|--setenv) shift 3 ;;
    --tmpfs|--proc|--dev|--json-status-fd) shift 2 ;;
    --*) shift ;;
    *) exec "$@" ;;
   esac
  done ;;
 lpstat)
  if test -f "$root/no-printer"; then exit 0; fi
  case "$1" in -e) printf 'Kitchen\nBrother\n' ;; -d) printf 'system default destination: Brother\n' ;; -l) if test -f "$root/hold-lpstat"; then set -C; : > "$root/lpstat-held"; exec /bin/sleep 30; fi
       printf 'printer Brother is idle\n\tDescription: Brother HL-L3270CDW\n' ;; esac ;;
 lpoptions) printf 'Duplex/Two-sided: *None DuplexNoTumble\nColorModel/Color: *RGB Gray\n' ;;
 lp)
  case "$*" in *hold.pdf*) set -C; : > "$root/lp-held"; exec /bin/sleep 30 ;; esac
  case "$*" in *stderr-cancelled.pdf*) printf 'cancelled\n' >&2; exit 1 ;; esac
  case "$*" in *refuse.pdf*) printf 'printer is paused\n' >&2; exit 1 ;; esac ;;
 pdfinfo) printf 'Pages: 3\n' ;;
 magick|montage|pdftoppm)
  last=; codec=png; page=1; previous=
  for arg do
   test "$previous" != -f || page=$arg
   case "$arg" in -jpeg) codec=jpg ;; -tiff) codec=tif ;; esac
   previous=$arg; last=$arg
  done
  if test "$last" = info:; then
   for arg do test "$arg" != -- || printf '800 600\n'; done
   exit 0
  fi
  test "$tool" != pdftoppm || last=$last.$codec
  case "$last" in jpg:/*|png:/*|webp:/*|avif:/*|heic:/*|tiff:/*|bmp:/*|pdf:/*) last=${last#*:} ;; esac
  case "$last" in "$root"/*) ;; *) exit 97 ;; esac
  set -C
  printf '%s %s\n' "$tool" "$page" > "$last"
  take=0
  for arg do
   if test "$arg" = --; then take=1; continue; fi
   if test "$take" = 1; then
    take=0; file=${arg%\[0\]}
    test ! -f "$file" || cat -- "$file" >> "$last"
   fi
  done ;;
 *) exit 99 ;;
esac
"#;

#[test]
fn print_protocol_helper() {
    let Some(root) = std::env::var_os("FLEA_PRINT_TEST_ROOT") else { return; };
    let root = PathBuf::from(root);
    assert!(crate::backend::testdir::removable(&root));
    std::process::exit(crate::backend::run::run());
}

struct Rig {
    dir: TestDir,
    child: Child,
    input: ChildStdin,
    lines: Receiver<String>,
}

impl Rig {
    fn new(stubs: bool) -> Self { Self::with_tools(stubs, None) }

    fn with_magick(script: &str) -> Self { Self::with_tools(true, Some(script)) }

    fn with_tools(stubs: bool, magick: Option<&str>) -> Self {
        use std::os::unix::process::CommandExt;
        let dir = TestDir::new("print-protocol");
        let bin = dir.dir("bin");
        if stubs {
            let script = dir.script("tools", STUB);
            let custom = magick.map(|body| dir.script("magick-stub", body));
            for name in ["lpstat", "lpoptions", "lp", "magick", "montage", "pdftoppm", "pdfinfo", "prlimit", "bwrap"] {
                let source = if name == "magick" { custom.as_ref().unwrap_or(&script) } else { &script };
                std::os::unix::fs::symlink(source, bin.join(name)).unwrap();
            }
            std::os::unix::fs::symlink("/usr/bin/true", bin.join("soffice")).unwrap();
        }
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", HELPER, "--nocapture"]).env("FLEA_PRINT_TEST_ROOT", dir.path());
        for (key, leaf) in [("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"), ("XDG_STATE_HOME", "state"), ("XDG_RUNTIME_DIR", "runtime"), ("XDG_DATA_HOME", "data")] {
            command.env(key, dir.dir(leaf));
        }
        if stubs { command.env("PATH", if magick.is_some() { bin.display().to_string() } else { format!("{}:/usr/bin:/bin", bin.display()) }); }
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).process_group(0).spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break; };
                if line.starts_with('{') { let _ = tx.send(line); }
            }
        });
        Self { dir, child, input, lines }
    }

    fn send(&mut self, line: &str) { writeln!(self.input, "{line}").unwrap(); }

    fn until(&mut self, kind: &str) -> Vec<String> {
        let end = Instant::now() + Duration::from_secs(30);
        let mut found = Vec::new();
        loop {
            let line = self.lines.recv_timeout(end.saturating_duration_since(Instant::now())).unwrap_or_else(|e| panic!("waiting for {kind}: {e}; {found:?}"));
            let done = field_str(&line, "t").as_deref() == Some(kind);
            found.push(line);
            if done { return found; }
        }
    }

    fn ask(&mut self, command: &str, end: &str) -> Vec<String> { self.send(command); self.until(end) }

    fn calls(&self) -> String { std::fs::read_to_string(self.dir.join("calls")).unwrap_or_default() }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = writeln!(self.input, r#"{{"c":"quit"}}"#);
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if self.child.try_wait().ok().flatten().is_some() { return; }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = crate::procsignal::signal_group(self.child.id() as i32, 9);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn print_protocol_lists_the_default_and_the_no_printer_state() {
    let mut rig = Rig::new(true);
    let replies = rig.ask(r#"{"c":"printoptions","id":41}"#, "printoptions");
    let line = replies.last().unwrap();
    assert!(field_bool(line, "ok"), "{line}");
    assert!(line.find("Brother").unwrap() < line.find("Kitchen").unwrap());
    assert!(field_bool(line, "duplex"));
    assert!(field_bool(line, "color"));
    rig.dir.file("no-printer", "yes");
    let replies = rig.ask(r#"{"c":"printoptions","id":42}"#, "printoptions");
    assert!(replies.last().unwrap().contains("No printer is set up."));
}

#[test]
fn print_input_fetches_an_office_document_and_passes_every_other_kind_through() {
    let (source, fetched) = (PathBuf::from("/in/report.docx"), PathBuf::from("/cache/report.pdf"));
    let (calls, cancel) = (std::cell::Cell::new(0), Arc::new(AtomicBool::new(false)));
    let fetch = |p: &std::path::Path, _| { calls.set(calls.get() + 1); assert_eq!(p, source); Ok(fetched.clone()) };
    assert_eq!(print_input_with(fetch, &source, printconvert::Kind::Office, &cancel), Ok(fetched.clone()));
    for kind in [printconvert::Kind::Pdf, printconvert::Kind::Image, printconvert::Kind::Text, printconvert::Kind::Other] {
        assert_eq!(print_input_with(fetch, &source, kind, &cancel), Ok(source.clone()));
    }
    assert_eq!(calls.get(), 1, "only the office document is fetched");
}

#[test]
fn print_protocol_submits_one_lp_per_file_and_keeps_refusals() {
    let mut rig = Rig::new(true);
    let image = rig.dir.file("-photo.png", "pixels");
    let pdf = rig.dir.file("refuse.pdf", "%PDF");
    let command = format!(r#"{{"c":"print","requestId":7,"paths":["{}","{}"],"printer":"Brother","printerName":"Brother HL-L3270CDW","copies":2,"pages":"1-3, 5","twoSided":true,"grayscale":true}}"#, image.display(), pdf.display());
    let replies = rig.ask(&command, "printdone");
    let receipts: Vec<_> = replies.iter().filter(|line| field_str(line, "t").as_deref() == Some("printed")).collect();
    assert_eq!(receipts.len(), 2, "{replies:?}");
    assert_eq!(field_str(receipts[0], "message").as_deref(), Some("Sent -photo.png to Brother HL-L3270CDW"));
    assert_eq!(field_str(receipts[1], "message").as_deref(), Some("refuse.pdf was not printed: printer is paused."));
    assert_eq!(field_usize(replies.last().unwrap(), "sent"), Some(1));
    let calls = rig.calls();
    let lp: Vec<_> = calls.lines().filter(|line| line.starts_with("lp\0")).collect();
    assert_eq!(lp.len(), 2);
    assert!(lp[0].contains("-o\0fit-to-page\0--\0"));
    assert!(!lp[0].contains("-P\0"));
    assert!(lp[1].contains("-P\x001-3,5\0"));
}

#[test]
fn print_protocol_converts_images_and_pdf_pages_in_order_without_overwrite() {
    let mut rig = Rig::new(true);
    let first = rig.dir.file("b.png", "first image\n");
    let next = rig.dir.file("a.png", "second image\n");
    let command = format!(r#"{{"c":"convertbatch","requestId":11,"paths":["{}","{}"],"format":"pdf","strip":true}}"#, first.display(), next.display());
    let replies = rig.ask(&command, "convertdone");
    assert!(field_bool(replies.last().unwrap(), "ok"), "{replies:?}");
    let output = rig.dir.join("b (converted).pdf");
    let contents = std::fs::read_to_string(&output).unwrap();
    assert!(contents.find("first image").unwrap() < contents.find("second image").unwrap());
    let again = rig.ask(&command, "convertdone");
    assert!(field_bool(again.last().unwrap(), "collision"));
    assert_eq!(std::fs::read_to_string(&output).unwrap(), contents);
    let pdf = rig.dir.file("report.pdf", "%PDF");
    for format in ["png", "jpg", "webp", "tiff"] {
        let command = format!(r#"{{"c":"convertbatch","requestId":12,"path":"{}","format":"{format}","pages":"3, 1"}}"#, pdf.display());
        let replies = rig.ask(&command, "convertdone");
        let done = replies.last().unwrap();
        assert!(field_bool(done, "ok"), "{done}");
        assert_eq!(crate::json::field_str_array(done, "paths"), [rig.dir.join(&format!("report (page 1).{format}")).to_string_lossy(), rig.dir.join(&format!("report (page 3).{format}")).to_string_lossy()]);
    }
    assert!(!rig.dir.join("report (page 2).png").exists());
    assert_eq!(std::fs::read_to_string(first).unwrap(), "first image\n");
    assert_eq!(std::fs::read_to_string(next).unwrap(), "second image\n");
}

#[test]
fn print_protocol_combines_layouts_and_exports_office_from_the_preview_cache() {
    let mut rig = Rig::new(true);
    let first = rig.dir.file("first.jpg", "first\n");
    let second = rig.dir.file("second.png", "second\n");
    for (layout, spacing) in [("Row", "Small"), ("Column", "Large"), ("Grid", "None")] {
        let command = format!(r#"{{"c":"combine","requestId":21,"paths":["{}","{}"],"layout":"{layout}","spacing":"{spacing}","background":"White","format":"png","name":"{layout}.png"}}"#, first.display(), second.display());
        let replies = rig.ask(&command, "combinedone");
        assert!(field_bool(replies.last().unwrap(), "ok"), "{replies:?}");
        assert!(rig.dir.join(&format!("{layout}.png")).is_file());
    }
    let document = rig.dir.file("letter.docx", "office");
    let cache_dir = rig.dir.dir("cache/flea/office");
    let cache = cache_dir.join(format!("{}.pdf", super::super::officepdf::Store::key(&document)));
    rig.dir.assert_contains(&cache);
    let file = std::fs::File::create(&cache).unwrap();
    (&file).write_all(b"%PDF cached preview").unwrap();
    file.set_modified(document.metadata().unwrap().modified().unwrap()).unwrap();
    let command = format!(r#"{{"c":"convertbatch","requestId":22,"path":"{}","format":"pdf"}}"#, document.display());
    let replies = rig.ask(&command, "convertdone");
    assert!(field_bool(replies.last().unwrap(), "ok"), "{replies:?}");
    assert_eq!(std::fs::read(rig.dir.join("letter (converted).pdf")).unwrap(), b"%PDF cached preview");
    let command = format!(r#"{{"c":"print","path":"{}","printer":"Brother"}}"#, document.display());
    let replies = rig.ask(&command, "printdone");
    assert!(field_bool(replies.last().unwrap(), "ok"), "{replies:?}");
    assert!(rig.calls().lines().any(|line| line.starts_with("lp\0") && line.contains(&cache.to_string_lossy().to_string())));
}

#[test]
fn vf4_print_options_carry_the_advertised_printer_name() {
    let mut rig = Rig::new(true);
    let replies = rig.ask(r#"{"c":"printoptions","id":41}"#, "printoptions");
    let line = replies.last().unwrap();
    assert_eq!(field_str(line, "printer").as_deref(), Some("Brother"));
    assert_eq!(field_str(line, "printerName").as_deref(), Some("Brother HL-L3270CDW"));
}

#[test]
fn vf4_r1_a_cancel_during_the_printer_name_query_is_not_an_ok_reply() {
    let mut rig = Rig::new(true);
    rig.dir.file("hold-lpstat", "");
    rig.send(r#"{"c":"printoptions","id":42}"#);
    const HELD_WAIT: Duration = Duration::from_secs(5);
    const HELD_POLL: Duration = Duration::from_millis(5);
    let entered = rig.dir.join("lpstat-held");
    let end = Instant::now() + HELD_WAIT;
    while !entered.is_file() && Instant::now() < end {
        std::thread::sleep(HELD_POLL);
    }
    assert!(entered.is_file(), "lpstat -l was reached before the cancel");
    rig.send(r#"{"c":"quit"}"#);
    let replies = rig.until("printoptions");
    let line = replies.last().unwrap();
    assert!(!field_bool(line, "ok"), "{replies:?}");
    assert_eq!(field_str(line, "err").as_deref(), Some("cancelled"));
}
