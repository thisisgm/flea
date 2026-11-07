use super::*;
use crate::backend::archivework::last_jail;
use crate::backend::proto::{parse_request, Request};
use flea::ui_logic_pv2::office::{parse, Wire};

#[path = "officesheets_guard_tests.rs"]
mod guard;

// The stand-in for soffice's CSV filter: the input lists its sheet names, $3 is option 12, and every CSV is named <stem>-<sheet>.csv.
const SHEETS: &str = r#"stem=$(basename "$1"); stem=${stem%.*}; mkdir -p "$2"; i=0
while IFS= read -r name; do i=$((i+1)); if [ "$3" = -1 ] || [ "$3" = "$i" ]; then printf 'Sheet,%s\nrow,%s\n' "$name" "$i" > "$2/$stem-$name.csv"; fi; done < "$1"
[ "$3" = -1 ] || [ "$3" -le "$i" ] || { echo 'Error: Please verify input parameters' >&2; exit 1; }"#;
// An order run that names a sheet the whole-workbook run never wrote.
const STRAY: &str = r#"mkdir -p "$2"; if [ "$3" = -1 ]; then printf 'a\n' > "$2/b-One.csv"; printf 'b\n' > "$2/b-Two.csv"; else printf 'c\n' > "$2/b-Other.csv"; fi"#;

fn sheets_rig(d: &TestDir, script: &'static str) -> (Rig, Arc<AtomicUsize>) {
    let mut r = rig(d, MAKE_PDF, CONVERT_TIMEOUT);
    let runs = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&runs);
    Arc::get_mut(&mut r.svc)
        .expect("a fresh rig owns its service")
        .sheet_build = Box::new(move |_, input, out, sheet| {
        seen.fetch_add(1, Ordering::SeqCst);
        [
            "/usr/bin/sh",
            "-c",
            script,
            "sh",
            &input.to_string_lossy(),
            &out.to_string_lossy(),
            &sheet.to_string(),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    });
    (r, runs)
}

impl Rig {
    fn ask_sheets(&self, wire: usize, path: &Path) -> String {
        let reg = self.claims.fetch_add(1, Ordering::SeqCst) + 1;
        self.svc.request(
            wire,
            reg,
            self.svc.seq.fetch_add(1, Ordering::Relaxed),
            path,
            true,
            None,
            &self.tx,
        );
        match self
            .rx
            .recv_timeout(BOUND)
            .expect("a request always answers")
        {
            OpMsg::Meta { line } | OpMsg::DetachedDone { line, .. } => line,
            _ => panic!("a request answers with Meta or DetachedDone"),
        }
    }
}

// The ready line read back through the viewer's own parser, so the backend and OfficeView agree on the shape.
fn sheets_of(line: &str) -> (Vec<String>, Vec<String>) {
    match parse(line).expect("the viewer parses the line").wire {
        Wire::Sheets { names, paths } => (names, paths),
        other => panic!("expected sheets, got {other:?} from {line}"),
    }
}

#[test]
fn officesheets_parses_as_the_office_request_in_sheets_mode() {
    let sheets = parse_request(r#"{"c":"officesheets","id":5,"path":"/d/budget.xlsx"}"#);
    assert!(
        matches!(sheets, Request::OfficePdf { id: 5, ref path, sheets: true } if path == "/d/budget.xlsx")
    );
    let pdf = parse_request(r#"{"c":"officepdf","id":4,"path":"/d/report.docx"}"#);
    assert!(matches!(
        pdf,
        Request::OfficePdf {
            id: 4,
            sheets: false,
            ..
        }
    ));
    let asked = flea::ui_logic_pv2::office::request_line(7, "/d/a \"b\".xlsx", true);
    assert!(
        matches!(parse_request(&asked), Request::OfficePdf { id: 7, ref path, sheets: true } if path == "/d/a \"b\".xlsx")
    );
}

#[test]
fn every_sheets_line_is_the_one_the_viewer_reads() {
    let sheets = vec![
        ("Monthly".to_string(), PathBuf::from("/c/k.sheets.1.csv")),
        ("Sav \"x\"".to_string(), PathBuf::from("/c/k.sheets.2.csv")),
    ];
    let line = done_line(3, &Done::Sheets(sheets));
    assert_eq!(
        line,
        r#"{"t":"officesheets","id":3,"state":"ready","names":["Monthly","Sav \"x\""],"paths":["/c/k.sheets.1.csv","/c/k.sheets.2.csv"]}"#
    );
    assert_eq!(
        sheets_of(&line),
        (
            vec!["Monthly".into(), "Sav \"x\"".into()],
            vec!["/c/k.sheets.1.csv".into(), "/c/k.sheets.2.csv".into()]
        )
    );
    assert_eq!(
        converting_line_of(true, 3),
        r#"{"t":"officesheets","id":3,"state":"converting"}"#
    );
    assert_eq!(
        failed_line_of(true, 3, "no", true),
        r#"{"t":"officesheets","id":3,"state":"failed","err":"no","unavailable":true}"#
    );
    assert_eq!(
        converting_line_of(false, 3),
        converting_line(3),
        "the PDF lines are unchanged"
    );
}

#[test]
fn a_workbook_answers_its_sheets_in_workbook_order_with_their_names() {
    let d = TestDir::new("officesheets-order");
    let (r, runs) = sheets_rig(&d, SHEETS);
    // Not alphabetical, so neither a directory listing nor a sort can stand in for the workbook's order.
    let src = d.file("budget.xlsx", "Zeta\nAlpha\nMid\n");
    assert_eq!(r.ask_sheets(1, &src), converting_line_of(true, 1));
    let (reg, line) = r.end();
    assert_eq!(reg, 1, "the end line frees the request's registry id");
    let (names, paths) = sheets_of(&line);
    assert_eq!(names, ["Zeta", "Alpha", "Mid"]);
    let bodies: Vec<String> = paths
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    assert_eq!(
        bodies,
        [
            "Sheet,Zeta\nrow,1\n",
            "Sheet,Alpha\nrow,2\n",
            "Sheet,Mid\nrow,3\n"
        ]
    );
    for p in &paths {
        assert!(
            p.starts_with(&r.cache.join(CACHE_LEAF).to_string_lossy().into_owned())
                && p.ends_with(".csv"),
            "{p}"
        );
        assert_eq!(
            mtime_of(Path::new(p)),
            mtime_of(&src),
            "each CSV is stamped with the workbook's mtime"
        );
    }
    assert_eq!(
        runs.load(Ordering::SeqCst),
        3,
        "one whole-workbook run, then one run per sheet but the last"
    );
    assert_eq!(
        (r.jobs(), r.fail_markers()),
        (0, 0),
        "no PDF conversion runs for a sheets request"
    );
}

#[test]
fn the_board_workbook_is_monthly_then_savings_in_one_run_each() {
    let d = TestDir::new("officesheets-budget");
    let (r, runs) = sheets_rig(&d, SHEETS);
    let src = d.file("budget.xlsx", "Monthly\nSavings\n");
    assert_eq!(r.ask_sheets(4, &src), converting_line_of(true, 4));
    assert_eq!(sheets_of(&r.end().1).0, ["Monthly", "Savings"]);
    assert_eq!(runs.load(Ordering::SeqCst), 2);
    let one = d.file("single.ods", "Only\n");
    assert_eq!(r.ask_sheets(5, &one), converting_line_of(true, 5));
    assert_eq!(sheets_of(&r.end().1).0, ["Only"]);
    assert_eq!(
        runs.load(Ordering::SeqCst),
        3,
        "a one-sheet workbook needs no order run"
    );
}

#[test]
fn a_large_workbook_lists_its_first_sheets_in_bounded_runs() {
    let d = TestDir::new("officesheets-cap");
    let (r, runs) = sheets_rig(&d, SHEETS);
    let names: Vec<String> = (1..=30).map(|i| format!("S{:02}", 31 - i)).collect();
    let src = d.file("big.xlsx", &(names.join("\n") + "\n"));
    r.ask_sheets(1, &src);
    assert_eq!(
        sheets_of(&r.end().1).0,
        names[..super::super::sheets::MAX_SHEETS]
    );
    assert_eq!(
        runs.load(Ordering::SeqCst),
        1 + super::super::sheets::MAX_SHEETS,
        "one whole-workbook run and one order run per listed sheet"
    );
}

#[test]
fn a_converted_workbook_is_served_from_the_cache_until_it_changes() {
    let d = TestDir::new("officesheets-cache");
    let (r, runs) = sheets_rig(&d, SHEETS);
    let src = d.file("budget.xlsx", "Monthly\nSavings\n");
    r.ask_sheets(1, &src);
    let first = r.end().1;
    let hit = r.ask_sheets(2, &src);
    assert_eq!(
        sheets_of(&hit),
        sheets_of(&first),
        "a second request is answered ready at once"
    );
    assert_eq!(runs.load(Ordering::SeqCst), 2, "the hit runs nothing");
    // The PDF of the same file is its own entry, so neither mode answers for the other.
    assert_eq!(r.ask(3, &src), converting_line(3));
    assert!(r.end().1.contains(r#""t":"officepdf""#));
    assert_eq!(sheets_of(&r.ask_sheets(4, &src)), sheets_of(&first));
    std::fs::write(&src, "Monthly\nSavings\nNotes\n").unwrap();
    let later = SystemTime::now() + Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&src)
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert_eq!(
        r.ask_sheets(5, &src),
        converting_line_of(true, 5),
        "a changed workbook converts again"
    );
    assert_eq!(sheets_of(&r.end().1).0, ["Monthly", "Savings", "Notes"]);
}
