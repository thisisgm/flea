//! The sheets cache and converter guards: torn caches, failures, stray sheets, the jail and the argv.
use super::*;

#[test]
fn a_torn_sheet_cache_is_a_miss() {
    let d = TestDir::new("officesheets-torn");
    let (r, runs) = sheets_rig(&d, SHEETS);
    let src = d.file("budget.xlsx", "Monthly\nSavings\n");
    r.ask_sheets(1, &src);
    let (_, paths) = sheets_of(&r.end().1);
    std::fs::remove_file(&paths[1]).unwrap();
    assert_eq!(
        r.ask_sheets(2, &src),
        converting_line_of(true, 2),
        "a manifest whose CSV is gone converts again"
    );
    assert_eq!(sheets_of(&r.end().1).1, paths);
    assert_eq!(runs.load(Ordering::SeqCst), 4);
}

#[test]
fn a_converter_failure_is_a_sheets_line_and_is_remembered() {
    let d = TestDir::new("officesheets-broken");
    let (r, runs) = sheets_rig(&d, BROKEN);
    let src = d.file("budget.xlsx", "Monthly\n");
    assert_eq!(r.ask_sheets(1, &src), converting_line_of(true, 1));
    let line = r.end().1;
    assert!(
        line.starts_with(r#"{"t":"officesheets","id":1,"state":"failed","err":""#)
            && line.contains("boom"),
        "{line}"
    );
    assert_eq!(
        r.ask_sheets(2, &src).replace(r#""id":2"#, r#""id":1"#),
        line,
        "the failure is remembered for the workbook"
    );
    let key = format!("{}{SHEETS_SUFFIX}", Store::key(&src));
    assert!(
        r.svc.store.fail_path(&key).is_file() && !r.svc.store.manifest_path(&key).exists(),
        "the marker sits under fail/flea, apart from the manifest"
    );
    assert_eq!((runs.load(Ordering::SeqCst), r.fail_markers()), (1, 1));
    assert_eq!(
        r.ask(3, &src),
        converting_line(3),
        "a sheets failure never answers the PDF request"
    );
}

#[test]
fn a_clean_exit_without_a_csv_is_a_recorded_failure() {
    let d = TestDir::new("officesheets-nocsv");
    let (r, _) = sheets_rig(&d, "exit 0");
    let src = d.file("budget.xlsx", "Monthly\n");
    r.ask_sheets(1, &src);
    let message = format!(
        "soffice wrote no sheets of {}",
        std::fs::canonicalize(&src).unwrap().display()
    );
    assert_eq!(r.end().1, failed_line_of(true, 1, &message, false));
    assert_eq!(r.fail_markers(), 1);
}

#[test]
fn an_order_run_naming_an_unknown_sheet_fails_the_workbook() {
    let d = TestDir::new("officesheets-stray");
    let (r, _) = sheets_rig(&d, STRAY);
    let src = d.file("b.xlsx", "x");
    r.ask_sheets(1, &src);
    let line = r.end().1;
    assert!(
        line.contains(r#""state":"failed""#) && line.contains("sheet 1"),
        "{line}"
    );
    assert!(
        !std::fs::read_dir(r.cache.join(CACHE_LEAF))
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().ends_with(".csv")),
        "nothing is published"
    );
}

#[test]
fn a_refused_jail_answers_failed_and_publishes_nothing() {
    let d = TestDir::new("officesheets-refused");
    let mut r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let refuse: Runner = Box::new(|argv, _, _, _| {
        Err(crate::backend::opsreq::op_err(
            "office",
            &argv[0],
            "the sandbox refused",
        ))
    });
    r.svc = Arc::new(Service::new(
        Store::new(&r.cache),
        Box::new(|| Some("/usr/bin/sh".to_string())),
        Box::new(convert_argv),
        refuse,
        CONVERT_TIMEOUT,
    ));
    let src = d.file("budget.xlsx", "Monthly\n");
    assert_eq!(r.ask_sheets(1, &src), converting_line_of(true, 1));
    let line = r.end().1;
    assert!(
        line.starts_with(r#"{"t":"officesheets","id":1,"state":"failed""#)
            && line.contains("the sandbox refused"),
        "{line}"
    );
    assert!(!std::fs::read_dir(r.cache.join(CACHE_LEAF))
        .unwrap()
        .flatten()
        .any(|e| e.path().extension().is_some_and(|x| x == "csv")));
}

#[test]
fn a_foreign_file_at_a_sheet_path_is_left_alone() {
    let d = TestDir::new("officesheets-foreign");
    let (r, runs) = sheets_rig(&d, SHEETS);
    let src = d.file("budget.xlsx", "Monthly\n");
    let key = format!("{}{SHEETS_SUFFIX}", Store::key(&src));
    let foreign = r.svc.store.csv_path(&key, 1);
    std::fs::create_dir_all(foreign.parent().unwrap()).unwrap();
    std::fs::write(&foreign, b"\x00\x01binary").unwrap();
    r.ask_sheets(1, &src);
    let line = r.end().1;
    assert!(
        line.contains(r#""state":"failed""#) && line.contains("foreign"),
        "{line}"
    );
    assert_eq!(std::fs::read(&foreign).unwrap(), b"\x00\x01binary");
    assert_eq!(r.fail_markers(), 0, "a local refusal is not remembered");
    assert_eq!(
        runs.load(Ordering::SeqCst),
        0,
        "the refusal comes before the first converter run"
    );
}

#[test]
fn the_cache_bound_counts_sheet_entries() {
    let d = TestDir::new("officesheets-prune");
    let (mut r, _) = sheets_rig(&d, SHEETS);
    Arc::get_mut(&mut r.svc).unwrap().store.entries = 3;
    for (i, name) in ["a.xlsx", "b.xlsx"].into_iter().enumerate() {
        let src = d.file(name, "One\nTwo\n");
        r.ask_sheets(i + 1, &src);
        assert!(r.end().1.contains(r#""state":"ready""#));
        // Coarse ctime ticks would tie the two workbooks, and prune evicts by ctime.
        std::thread::sleep(Duration::from_millis(50));
    }
    let left = std::fs::read_dir(r.cache.join(CACHE_LEAF))
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_file())
        .count();
    assert!(
        left <= 3,
        "the figure store's bound covers manifests and CSVs, {left} left"
    );
    let newest = d.path().join("b.xlsx");
    assert_eq!(
        r.ask_sheets(3, &newest).matches(".csv").count(),
        2,
        "the workbook just published survives its own prune and is a hit"
    );
}

#[test]
fn serve_refuses_a_sheets_request_for_a_document_before_any_job() {
    let mime = Db::from_str("50:application/vnd.oasis.opendocument.text:*.odt\n");
    let (tx, _rx) = channel();
    let mut ops = crate::backend::opsdispatch::Ops::new(tx);
    let mut out = Vec::new();
    serve(
        &mut out,
        &mut ops,
        &mime,
        &Aliases::from_str(""),
        6,
        "/x/notes.odt",
        true,
    );
    assert_eq!(
        String::from_utf8(out).unwrap(),
        failed_line_of(true, 6, "that file is not a spreadsheet", true) + "\n"
    );
    assert!(ops.detached.is_empty(), "a refusal claims no registry id");
}

#[test]
fn only_spreadsheets_are_asked_for_sheets() {
    let mime = Db::from_str("50:application/vnd.openxmlformats-officedocument.spreadsheetml.sheet:*.xlsx\n50:application/vnd.oasis.opendocument.spreadsheet:*.ods\n50:application/vnd.ms-excel:*.xls\n50:application/vnd.oasis.opendocument.text:*.odt\n");
    let aliases = Aliases::from_str("");
    for name in ["/x/a.xlsx", "/x/a.ods", "/x/a.xls"] {
        assert!(is_sheet(name, &mime, &aliases), "{name}");
    }
    assert!(!is_sheet("/x/a.odt", &mime, &aliases) && is_office("/x/a.odt", &mime, &aliases));
}

// The production argv: LibreOffice's CSV filter in UTF-8, values as shown, option 12 naming the sheet, inside the office jail.
#[test]
fn the_sheet_argv_is_the_csv_filter_one_sheet_at_a_time_in_the_office_jail() {
    let d = TestDir::new("officesheets-argv");
    let source = d.file("budget.xlsx", "x");
    let mut work = Work::new(d.path(), "office-sheets-argv").unwrap();
    let out = work.dir.join("s2");
    let office = sheets_argv("/nonexistent/soffice", &source, &out, 2);
    let filter = "csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,2";
    assert_eq!(
        office,
        [
            "/nonexistent/soffice",
            "--headless",
            "--norestore",
            "--nolockcheck",
            "-env:UserInstallation=file:///tmp/flea-office-profile",
            "--convert-to",
            filter,
            "--outdir",
            &out.to_string_lossy(),
            &source.to_string_lossy()
        ]
    );
    assert!(
        sheets_argv("/s", &source, &out, -1)[6].ends_with(",-1"),
        "the whole-workbook run"
    );
    let _ = jail(office.clone(), &source, &mut work, &AtomicBool::new(false));
    let full = last_jail().expect("the production office runner builds its jail argv");
    assert_eq!(&full[..3], ["prlimit", "--as=8589934592", "bwrap"]);
    assert_eq!(&full[full.len() - office.len()..], office.as_slice());
}

#[test]
fn csv_cache_documents_are_ours_only_when_they_are_text() {
    let d = TestDir::new("officesheets-cachewrite");
    let text = d.file("k.sheets.1.csv", "a,b\n1,2\n");
    let binary = d.dir("x").join("k.sheets.2.csv");
    std::fs::write(&binary, b"PK\x03\x04\x00").unwrap();
    assert!(crate::cachewrite::allowed(&text) && !crate::cachewrite::allowed(&binary));
    assert!(crate::cachewrite::allowed(&d.path().join("absent.csv")));
}
