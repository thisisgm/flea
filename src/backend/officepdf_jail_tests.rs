use super::*;
use crate::backend::archivework::{last_jail, office_failure_message, run_boxed_cancellable_capped};
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

#[test]
fn the_office_jail_has_no_cpu_cap_and_its_own_address_space_cap_and_every_other_jail_keeps_both() {
    let d = TestDir::new("officepdf-cap");
    let mut work = Work::new(d.path(), "office-cap").unwrap();
    let inner = vec!["/usr/bin/true".to_string()];
    let _ = jail(inner.clone(), d.path(), &mut work, &AtomicBool::new(false));
    let office = last_jail().expect("the production office runner builds its jail argv");
    assert_eq!(&office[..3], ["prlimit", "--as=8589934592", "bwrap"], "prlimit stays outermost with no CPU cap: the 60 s wall clock bounds the converter");
    assert_eq!(&office[office.len() - inner.len()..], inner.as_slice());
    let _ = run_boxed_cancellable_capped("convert", inner.clone(), d.path(), &mut work, &AtomicBool::new(false));
    assert_eq!(&last_jail().unwrap()[..4], ["prlimit", "--cpu=30", "--as=2147483648", "bwrap"], "image convert keeps the shared cap");
    let thumbnail = sandbox::wrap(&inner, Path::new("/in/a.mp4"), Path::new("/out"));
    assert_eq!(&thumbnail[..4], ["prlimit", "--cpu=30", "--as=2147483648", "bwrap"], "the thumbnail jail keeps the shared cap");
    let print = sandbox::wrap_compile(&inner, &[], &work.dir);
    assert_eq!(&print[..4], ["prlimit", "--cpu=30", "--as=2147483648", "bwrap"], "the print and convert image tools (ImageMagick, pdfinfo, combine) keep the shared cap; print's office leg is the office service above");
    let font_cache = ["--ro-bind-try", "/var/cache/fontconfig", "/var/cache/fontconfig"];
    assert!(office.windows(3).any(|w| w == font_cache), "the office jail reads the system font cache: {office:?}");
    assert!(!thumbnail.windows(3).any(|w| w == font_cache) && !print.windows(3).any(|w| w == font_cache), "only the office jail gains the bind");
}

#[test]
fn a_clean_exit_without_a_pdf_is_a_recorded_failure_naming_soffice_and_the_input() {
    let d = TestDir::new("officepdf-nopdf");
    let r = rig(&d, "exit 0", CONVERT_TIMEOUT);
    let src = d.file("a.docx", "doc");
    assert_eq!(r.ask(1, &src), converting_line(1));
    let message = format!("soffice wrote no PDF of {}", std::fs::canonicalize(&src).unwrap().display());
    assert_eq!(r.end().1, failed_line(1, &message, false));
    assert_eq!(r.ask(2, &src), failed_line(2, &message, false), "the failure is remembered for the file");
    assert_eq!((r.jobs(), r.fail_markers()), (1, 1));
}

// The argv the production office jail hands to spawn, read back through last_jail, so no soffice is needed.
#[test]
fn the_office_jail_runs_the_office_argv_inside_the_full_sandbox() {
    let d = TestDir::new("officepdf-argv");
    let source = d.file("letter.rtf", "{\\rtf1\\ansi fixture}");
    let mut work = Work::new(d.path(), "office-argv").unwrap();
    let office = argv("/nonexistent/soffice", &source, &work.dir);
    let _ = jail(office.clone(), &source, &mut work, &AtomicBool::new(false));
    let full = last_jail().expect("the production office runner builds its jail argv");
    let (src, out) = (source.to_string_lossy().into_owned(), work.dir.to_string_lossy().into_owned());
    assert_eq!(&full[..3], ["prlimit", "--as=8589934592", "bwrap"]);
    assert!(full.iter().any(|a| a == "--unshare-all"), "{full:?}");
    assert!(full.iter().any(|a| a == "--clearenv"), "{full:?}");
    assert!(full.windows(3).any(|w| w == ["--ro-bind", src.as_str(), src.as_str()]), "{full:?}");
    assert!(full.windows(3).any(|w| w == ["--bind", out.as_str(), out.as_str()]), "{full:?}");
    assert_eq!(&full[full.len() - office.len() - 2..][..2], ["--json-status-fd", "7"]);
    assert_eq!(&full[full.len() - office.len()..], office.as_slice());
}

// The 0.4.0 office matrix on minipc: a 200,000-row workbook killed at the CPU cap (status 137) told the user "/.cache/fontconfig", fontconfig's last line.
#[test]
fn a_sigkill_is_named_by_its_signal_and_fontconfig_lines_are_never_the_diagnosis() {
    let fontconfig = "Fontconfig error: No writable cache directories\n\t/var/cache/fontconfig\n\t/.cache/fontconfig\n";
    let killed = "the office tool stopped before it finished (signal SIGKILL)";
    assert_eq!(office_failure_message(&ExitStatus::from_raw(137 << 8), fontconfig), killed, "bwrap's 128+9");
    assert_eq!(office_failure_message(&ExitStatus::from_raw(9), "Error: something printed late\n"), killed, "a real signal beats any stderr line");
    let aborted = ExitStatus::from_raw(134 << 8);
    assert_eq!(office_failure_message(&aborted, &format!("{fontconfig}terminate called after throwing an instance of 'std::bad_alloc'\n")), "terminate called after throwing an instance of 'std::bad_alloc'", "a converter that aborts itself says why");
    assert_eq!(office_failure_message(&aborted, fontconfig), "the office tool stopped before it finished (signal SIGABRT)", "with nothing else printed the signal names it");
    let exited = ExitStatus::from_raw(1 << 8);
    assert_eq!(office_failure_message(&exited, fontconfig), "the office tool could not finish (exit status 1)", "fontconfig alone says nothing");
    let loaded = format!("Error: source file could not be loaded\n{fontconfig}");
    assert_eq!(office_failure_message(&exited, &loaded), "Error: source file could not be loaded", "the converter's own line survives fontconfig's after it");
    assert_eq!(office_failure_message(&exited, "\tindented but no fontconfig head\n"), "indented but no fontconfig head", "only lines under fontconfig's head are dropped");
}
