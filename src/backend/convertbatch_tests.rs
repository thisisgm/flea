use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn pdf_rendering_uses_150_dpi_and_only_the_chosen_page() {
    assert_eq!(pdf_argv(Path::new("/work/in.pdf"), Path::new("/work/page"), 5, "jpg"),
        ["pdftoppm", "-r", "150", "-f", "5", "-l", "5", "-singlefile", "-jpeg", "--", "/work/in.pdf", "/work/page"]);
    assert!(pdf_argv(Path::new("/in.pdf"), Path::new("/out"), 1, "tiff").contains(&"-tiff".into()));
    assert!(pdf_argv(Path::new("/in.pdf"), Path::new("/out"), 1, "webp").contains(&"-png".into()));
}

#[test]
fn multiple_images_keep_the_listing_order_and_combine_by_default() {
    let ask = Ask::parse(r#"{"paths":["/x/b.png","/x/a.jpg"],"format":"pdf"}"#);
    assert!(ask.combine);
    assert_eq!(image_argv(&ask.paths, Path::new("/out.pdf"), false),
        ["magick", "--", "/x/b.png[0]", "--", "/x/a.jpg[0]", "pdf:/out.pdf"]);
    assert_eq!(image_argv(&ask.paths, Path::new("/out.tiff"), true),
        ["magick", "--", "/x/b.png", "--", "/x/a.jpg", "-strip", "tiff:/out.tiff"]);
    assert!(!Ask::parse(r#"{"combine":false}"#).combine);
}

#[test]
fn a_raw_named_page_source_keeps_its_bytes_beside_the_page_suffix() {
    use std::os::unix::ffi::OsStrExt;
    let input = PathBuf::from(std::ffi::OsStr::from_bytes(b"/x/scan-\xff.png"));
    let args = image_argv(&[input], Path::new("/out.pdf"), false);
    assert_eq!(
        flea::bytename::text_name(&args[2]).as_bytes(),
        b"/x/scan-\xff.png[0]"
    );
}

#[test]
fn image_encoder_is_explicit_for_every_supported_target() {
    for format in ["jpg", "png", "webp", "avif", "heic", "tiff", "bmp", "pdf"] {
        let output = PathBuf::from(format!("/work/output-0.{format}"));
        let args = image_argv(&[PathBuf::from("/work/input-0.png")], &output, true);
        assert_eq!(args.last().unwrap(), &format!("{format}:/work/output-0.{format}"),
            "filename inference can silently fall back to the source codec when {format} has no encoder");
    }
}

#[test]
fn output_collisions_include_dangling_links_and_never_choose_the_original() {
    let d = TestDir::new("print-convert-names");
    let src = d.file("photo.png", "original");
    let mut ask = Ask::parse(&format!(r#"{{"paths":["{}"],"format":"pdf"}}"#, src.display()));
    assert_eq!(image_outputs(&ask).unwrap(), [d.join("photo (converted).pdf")]);
    d.file("photo (converted).pdf", "keep");
    assert_eq!(image_outputs(&ask).unwrap(), [d.join("photo (converted).pdf")], "a taken name is stepped past at publish, not refused");
    ask.format = "png".into();
    assert_eq!(image_outputs(&ask).unwrap(), [d.join("photo (stripped).png")]);
    let page = d.join("report (page 1).png");
    std::os::unix::fs::symlink(d.join("missing"), &page).unwrap();
    assert_eq!(destination(&src, "report (page 1).png".into(), true).unwrap(), d.join("report (page 1) (converted).png"));
    assert_eq!(std::fs::read_to_string(src).unwrap(), "original");
}

#[test]
fn different_inputs_that_would_share_one_output_name_are_refused() {
    let d = TestDir::new("print-convert-duplicate-output");
    let a = d.file("photo.png", "a");
    let b = d.file("photo.jpg", "b");
    let mut ask = Ask::parse(r#"{"format":"pdf","combine":false}"#);
    ask.paths = vec![a, b];
    assert!(image_outputs(&ask).unwrap_err().contains("same output name"));
}

// S1B-8: a second Convert of the same file lands under Duplicate's next free copy name and leaves the first whole.
#[test]
fn a_taken_output_lands_under_the_next_free_copy_name() {
    let d = TestDir::new("print-convert-free-name");
    let source = d.file("photo.png", "original");
    let dest = d.join("photo (converted).jpg");
    d.file("photo (converted).jpg", "first");
    d.file("photo (converted) copy.jpg", "second");
    let made = d.file("made.jpg", "third");
    assert_eq!(
        land(&made, &dest, &AtomicBool::new(false)).unwrap(),
        d.join("photo (converted) copy 2.jpg")
    );
    assert_eq!(
        std::fs::read_to_string(d.join("photo (converted) copy 2.jpg")).unwrap(),
        "third"
    );
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "first");
    assert_eq!(
        std::fs::read_to_string(d.join("photo (converted) copy.jpg")).unwrap(),
        "second"
    );
    let raced = d.file("raced.jpg", "raced");
    assert_eq!(
        printtool::publish(&raced, &dest, &AtomicBool::new(false)).unwrap_err(),
        "already exists",
        "land steps past a name taken meanwhile only on this exact refusal"
    );
    let fresh = d.file("fresh.jpg", "fourth");
    assert_eq!(
        land(&fresh, &d.join("other (converted).jpg"), &AtomicBool::new(false)).unwrap(),
        d.join("other (converted).jpg"),
        "a free name lands as asked"
    );
    assert_eq!(std::fs::read_to_string(source).unwrap(), "original");
}

// S1B-7: each published output journals as made from its source, so undo removes it only while it is untouched.
#[test]
fn a_published_output_journals_a_step_undo_reverses_once() {
    let d = TestDir::new("print-convert-journal");
    let source = d.file("photo.png", "original");
    let first = d.file("photo (converted).jpg", "converted");
    let second = d.file("photo (converted) copy.jpg", "converted again");
    let mut journal = crate::backend::undo::Journal::new();
    journal.push(crate::backend::undo::Entry {
        op: "convert".into(),
        steps: step(&source, &first).into_iter().collect(),
    });
    journal.push(crate::backend::undo::Entry {
        op: "convert".into(),
        steps: step(&source, &second).into_iter().collect(),
    });
    std::fs::write(&first, "edited since").unwrap();
    assert_eq!(
        journal.undo().expect("the untouched output goes"),
        "convert"
    );
    assert!(!second.exists() && first.exists() && source.exists());
    let refused = journal.undo().expect_err("an output edited since stays");
    assert!(refused.msg.contains("left it in place"), "{}", refused.msg);
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "edited since");
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "original");
}

// A source moved or deleted while the tool ran still leaves its output journalled, so z never skips to an older write.
#[test]
fn an_output_whose_source_is_gone_is_still_journalled() {
    let d = TestDir::new("print-convert-source-gone");
    let source = d.file("photo.png", "original");
    let output = d.file("photo (converted).jpg", "converted");
    std::fs::remove_file(&source).unwrap();
    let mut journal = crate::backend::undo::Journal::new();
    journal.push(crate::backend::undo::Entry {
        op: "convert".into(),
        steps: step(&source, &output).into_iter().collect(),
    });
    assert_eq!(journal.undo().expect("the output still goes"), "convert");
    assert!(!output.exists());
}

#[test]
fn numbered_image_outputs_are_ordered_and_a_taken_page_name_waits_for_publish() {
    let d = TestDir::new("print-convert-pages");
    let source = d.file("scan.tiff", "original pages");
    let work = d.dir("stage");
    let staged = work.join("output.png");
    let dest = d.join("scan (converted).png");
    for scene in [10, 2] { d.file(&format!("stage/output-{scene}.png"), &scene.to_string()); }
    let pages = image_results(&staged, &dest, &source).unwrap();
    assert_eq!(pages.iter().map(|(path, _)| std::fs::read_to_string(path).unwrap()).collect::<Vec<_>>(), ["2", "10"]);
    assert_eq!(pages[0].1, d.join("scan (converted) (page 1).png"));
    assert_eq!(pages[1].1, d.join("scan (converted) (page 2).png"));
    d.file("scan (converted) (page 2).png", "keep");
    assert_eq!(image_results(&staged, &dest, &source).unwrap(), pages, "a taken page name is stepped past at publish, not refused");
    assert!(!pages[0].1.exists());
    assert!(pages.iter().all(|(path, _)| path.is_file()), "the staged pages stay unpublished");
    assert_eq!(std::fs::read_to_string(&pages[1].1).unwrap(), "keep");
    assert_eq!(std::fs::read_to_string(source).unwrap(), "original pages");
}

#[test]
fn an_expired_menu_and_mixed_kinds_run_no_tools() {
    let d = TestDir::new("print-convert-stale");
    let path = d.file("photo.png", "a");
    let mut ask = Ask::parse(r#"{"format":"pdf","requestId":17}"#);
    ask.paths = vec![path];
    let mime = Db::from_str("50:image/png:*.png\n");
    let (tx, _) = std::sync::mpsc::channel();
    let (line, steps) = run(ask, Err("Selected items changed; reopen the menu.".into()), 3, &mime, &Aliases::from_str(""), Arc::new(AtomicBool::new(false)), &tx);
    assert!(steps.is_empty(), "a refused convert journals nothing");
    assert!(!crate::json::field_bool(&line, "ok"));
    assert_eq!(crate::json::field_usize(&line, "requestId"), Some(17));
    assert!(line.contains("Selected items changed"));
    assert!(!d.join("photo (converted).pdf").exists());
}

// A PDF page whose plain name and fallback are both taken lands under the fallback's copy name, never the plain name's.
#[test]
fn a_taken_page_fallback_lands_under_its_copy_name() {
    let d = TestDir::new("print-convert-page-copy");
    let src = d.file("report.pdf", "original");
    d.file("report (page 1).png", "plain");
    d.file("report (page 1) (converted).png", "fallback");
    let dest = destination(&src, "report (page 1).png".into(), true).unwrap();
    assert_eq!(dest, d.join("report (page 1) (converted).png"));
    let made = d.file("made.png", "page");
    let copy = d.join("report (page 1) (converted) copy.png");
    assert_eq!(land(&made, &dest, &AtomicBool::new(false)).unwrap(), copy);
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), "page");
    assert!(!made.exists(), "the staged page is moved, not copied");
    assert_eq!(
        std::fs::read_to_string(d.join("report (page 1).png")).unwrap(),
        "plain"
    );
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "fallback");
}

#[test]
fn a_taken_output_steps_through_duplicates_own_copy_names() {
    use super::printconvert::copy_path;
    for name in [
        "photo (converted).jpg",
        "backup.tar.zst",
        ".bashrc",
        "notes",
    ] {
        let output = Path::new("/x").join(name);
        assert_eq!(copy_path(&output, 0).unwrap(), output);
        for n in [1, 2, 999] {
            let duplicate =
                output.with_file_name(crate::backend::ops::copy_name(&output, "copy", n).unwrap());
            assert_eq!(copy_path(&output, n).unwrap(), duplicate, "{name} {n}");
        }
    }
}
