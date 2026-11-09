use super::*;

const WRITER: &str = "writer_pdf_Export";

// The rig with a converter that records the --convert-to target it was given and fails a first-pages run when asked to.
fn recording(d: &TestDir, head_fails: bool) -> (Rig, Arc<Mutex<Vec<String>>>) {
    let mut r = rig(d, MAKE_PDF, CONVERT_TIMEOUT);
    let targets = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&targets);
    let build: Build = Box::new(move |_, target, input, out| {
        seen.lock().unwrap().push(target.to_string());
        let script = if head_fails && target != "pdf" {
            BROKEN
        } else {
            MAKE_PDF
        };
        [
            "/usr/bin/sh",
            "-c",
            script,
            "sh",
            &input.to_string_lossy(),
            &out.to_string_lossy(),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    });
    let resolve: Resolve = Box::new(|| Some("/usr/bin/sh".to_string()));
    r.svc = Arc::new(Service::new(
        Store::new(&r.cache),
        resolve,
        build,
        Box::new(unjailed),
        CONVERT_TIMEOUT,
    ));
    (r, targets)
}

// Rig::ask for a preview: the first line the request leaves.
fn ask_head(r: &Rig, wire: usize, path: &Path) -> String {
    let reg = r.claims.fetch_add(1, Ordering::SeqCst) + 1;
    r.svc.request(
        wire,
        reg,
        r.svc.seq.fetch_add(1, Ordering::Relaxed),
        path,
        false,
        Some(WRITER),
        &r.tx,
    );
    match r.rx.recv_timeout(BOUND).expect("a request always answers") {
        OpMsg::Meta { line } | OpMsg::DetachedDone { line, .. } => line,
        _ => panic!("a request answers with Meta or DetachedDone"),
    }
}

#[test]
fn a_preview_converts_the_first_pages_under_its_own_key_and_print_converts_the_whole_document() {
    let d = TestDir::new("officehead-split");
    let (r, targets) = recording(&d, false);
    let src = d.file("long.docx", "doc");
    let head = r
        .svc
        .store
        .pdf_path(&format!("{}{HEAD_SUFFIX}", Store::key(&src)));
    assert_eq!(ask_head(&r, 1, &src), converting_line(1));
    let line = r.end().1;
    assert_eq!(line, ready_line(1, &head));
    assert!(
        line.ends_with(r#","first":30}"#),
        "the viewer is told the PDF is the first 30 pages: {line}"
    );
    let whole = r
        .svc
        .fetch(&src, Arc::new(AtomicBool::new(false)))
        .expect("print converts");
    assert_eq!(
        whole,
        r.svc.store.pdf_path(&Store::key(&src)),
        "print never takes the preview's cut"
    );
    assert!(
        !ready_line(2, &whole).contains("first"),
        "a whole PDF names no cut"
    );
    assert_eq!(
        ask_head(&r, 3, &src),
        ready_line(3, &head),
        "the next preview is a hit on its own key"
    );
    let want = [
        r#"pdf:writer_pdf_Export:{"PageRange":{"type":"string","value":"1-30"}}"#,
        "pdf",
    ];
    assert_eq!(
        *targets.lock().unwrap(),
        want,
        "one first-pages run, one whole run, nothing for the hit"
    );
}

#[test]
fn a_failed_preview_is_its_own_marker_and_print_still_converts() {
    let d = TestDir::new("officehead-failure");
    let (r, _) = recording(&d, true);
    let src = d.file("long.docx", "doc");
    assert_eq!(ask_head(&r, 1, &src), converting_line(1));
    assert_eq!(r.end().1, failed_line(1, "boom", false));
    assert_eq!(r.fail_markers(), 1);
    assert_eq!(
        r.svc.fetch(&src, Arc::new(AtomicBool::new(false))),
        Ok(r.svc.store.pdf_path(&Store::key(&src))),
        "a preview's failure never refuses print"
    );
    assert_eq!(
        ask_head(&r, 2, &src),
        failed_line(2, "boom", false),
        "the preview's failure stays remembered for the preview"
    );
}

#[test]
fn the_export_filter_follows_the_documents_group() {
    let mime = Db::from_str("50:application/vnd.oasis.opendocument.text:*.odt\n50:text/rtf:*.rtf\n50:application/vnd.ms-excel:*.xls\n50:application/vnd.openxmlformats-officedocument.presentationml.presentation:*.pptx\n50:application/vnd.oasis.opendocument.spreadsheet:*.ods\n");
    let aliases = Aliases::from_str("text/rtf application/rtf\n");
    let filter = |name| head::export_filter(name, &mime, &aliases);
    assert_eq!(
        [
            filter("/x/a.odt"),
            filter("/x/a.rtf"),
            filter("/x/a.xls"),
            filter("/x/a.ods"),
            filter("/x/a.pptx")
        ],
        [
            "writer_pdf_Export",
            "writer_pdf_Export",
            "calc_pdf_Export",
            "calc_pdf_Export",
            "impress_pdf_Export"
        ]
    );
}

#[test]
fn preview_names_are_told_apart_and_pruned_as_ours() {
    assert!(
        head::is_head(Path::new("/c/0123.head30.pdf")) && !head::is_head(Path::new("/c/0123.pdf"))
    );
    for (ext, ours) in [
        ("head30.pdf", true),
        ("head30.txt", true),
        ("head8.pdf", true),
        ("head.pdf", false),
        ("headx.pdf", false),
        ("head30.csv", false),
    ] {
        assert_eq!(head_extension(ext), ours, "{ext}");
    }
    assert_eq!(
        HEAD_SUFFIX,
        format!(".head{}", head::PREVIEW_PAGES),
        "the suffix names the cut it holds"
    );
}
