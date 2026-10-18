use super::*;
use crate::backend::archivework::{last_jail, run_boxed_cancellable_capped};

#[test]
fn the_office_jail_has_its_own_address_space_cap_and_every_other_jail_keeps_two_gib() {
    let d = TestDir::new("officepdf-cap");
    let mut work = Work::new(d.path(), "office-cap").unwrap();
    let inner = vec!["/usr/bin/true".to_string()];
    let _ = jail(inner.clone(), d.path(), &mut work, &AtomicBool::new(false));
    let office = last_jail().expect("the production office runner builds its jail argv");
    assert_eq!(&office[..4], ["prlimit", "--cpu=30", "--as=8589934592", "bwrap"], "prlimit stays outermost with the CPU cap unchanged");
    assert_eq!(&office[office.len() - inner.len()..], inner.as_slice());
    let _ = run_boxed_cancellable_capped("convert", inner.clone(), d.path(), &mut work, &AtomicBool::new(false));
    assert_eq!(&last_jail().unwrap()[..4], ["prlimit", "--cpu=30", "--as=2147483648", "bwrap"], "image convert keeps the shared cap");
    let thumbnail = sandbox::wrap(&inner, Path::new("/in/a.mp4"), Path::new("/out"));
    assert_eq!(&thumbnail[..4], ["prlimit", "--cpu=30", "--as=2147483648", "bwrap"], "the thumbnail jail keeps the shared cap");
    let print = sandbox::wrap_compile(&inner, &[], &work.dir);
    assert_eq!(&print[..4], ["prlimit", "--cpu=30", "--as=2147483648", "bwrap"], "the print and convert image tools (ImageMagick, pdfinfo, combine) keep the shared cap; print's office leg is the office service above");
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
    assert_eq!(&full[..4], ["prlimit", "--cpu=30", "--as=8589934592", "bwrap"]);
    assert!(full.iter().any(|a| a == "--unshare-all"), "{full:?}");
    assert!(full.iter().any(|a| a == "--clearenv"), "{full:?}");
    assert!(full.windows(3).any(|w| w == ["--ro-bind", src.as_str(), src.as_str()]), "{full:?}");
    assert!(full.windows(3).any(|w| w == ["--bind", out.as_str(), out.as_str()]), "{full:?}");
    assert_eq!(&full[full.len() - office.len() - 2..][..2], ["--json-status-fd", "7"]);
    assert_eq!(&full[full.len() - office.len()..], office.as_slice());
}
