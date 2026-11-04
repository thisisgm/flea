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
    assert!(image_outputs(&ask).unwrap_err().contains("already exists"));
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

#[test]
fn numbered_image_outputs_are_ordered_and_collisions_refuse_before_publication() {
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
    assert!(image_results(&staged, &dest, &source).unwrap_err().contains("already exists"));
    assert!(!pages[0].1.exists());
    assert!(pages.iter().all(|(path, _)| path.is_file()), "all staged pages remain unpublished");
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
    let line = run(ask, Err("Selected items changed; reopen the menu.".into()), 3, &mime, &Aliases::from_str(""), Arc::new(AtomicBool::new(false)), &tx);
    assert!(!crate::json::field_bool(&line, "ok"));
    assert_eq!(crate::json::field_usize(&line, "requestId"), Some(17));
    assert!(line.contains("Selected items changed"));
    assert!(!d.join("photo (converted).pdf").exists());
}
