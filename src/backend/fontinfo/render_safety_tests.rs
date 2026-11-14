use super::*;

#[test]
fn a_bad_ask_runs_no_job() {
    let d = TestDir::new("fontbad");
    let rig = Rig::new();
    let path = font(&d, "a.ttf");
    let mut relative = ask(&path);
    relative.path = PathBuf::from("a.ttf");
    let mut colour = ask(&path);
    colour.fg = "red".into();
    let mut sizes = ask(&path);
    sizes.sizes = vec![33, 22];
    let mut huge = ask(&path);
    huge.sizes = vec![33, 22, 100000];
    for bad in [
        relative,
        colour,
        sizes,
        huge,
        ask(&d.join("missing.ttf")),
        ask(d.path()),
    ] {
        let line = rig.run(&bad, &d.join("cache"));
        assert!(line.contains(r#""err":"#), "{}", line);
    }
    assert_eq!(rig.jobs.get(), 0);
}

#[test]
fn a_failed_job_or_a_wrong_output_leaves_nothing_in_the_cache() {
    let d = TestDir::new("fontfail");
    let rig = Rig::new();
    let path = font(&d, "a.ttf");
    let root = d.join("cache");
    let failed = answer(
        &ask(&path),
        &rig.mime,
        &rig.aliases,
        &root,
        "magick",
        &mut |_, _, _| {
            Err(op_err(
                "font",
                "magick",
                "the font tool exited with status 1",
            ))
        },
    );
    assert_eq!(
        failed,
        r#"{"t":"fontlooked","id":4,"err":"the font tool exited with status 1"}"#
    );
    let wrote_text = answer(
        &ask(&path),
        &rig.mime,
        &rig.aliases,
        &root,
        "magick",
        &mut |_, _, work| {
            for i in 0..SAMPLES {
                std::fs::write(work.dir.join(format!("{}.png", i)), b"not a png at all")
                    .expect("write");
            }
            Ok(())
        },
    );
    assert!(wrote_text.contains("did not draw"), "{}", wrote_text);
    let wrote_two = answer(
        &ask(&path),
        &rig.mime,
        &rig.aliases,
        &root,
        "magick",
        &mut |_, _, work| {
            std::fs::write(work.dir.join("0.png"), PNG).expect("write");
            std::fs::write(work.dir.join("1.png"), PNG).expect("write");
            Ok(())
        },
    );
    assert!(wrote_two.contains("did not draw"), "{}", wrote_two);
    let left: Vec<_> = std::fs::read_dir(&root)
        .expect("root")
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert!(left.is_empty(), "a failed job left {:?}", left);
}

#[test]
fn a_symlink_planted_at_a_cache_path_is_never_read_or_followed() {
    let d = TestDir::new("fontplant");
    let rig = Rig::new();
    let path = font(&d, "a.ttf");
    let root = d.join("cache");
    let first = rig.run(&ask(&path), &root);
    // A real PNG, so only the symlink itself can make it a miss.
    let victim = d.join("victim.png");
    std::fs::write(&victim, [PNG, b"keep me"].concat()).expect("victim");
    let planted = PathBuf::from(&files_of(&first)[1]);
    std::fs::remove_file(&planted).expect("remove");
    std::os::unix::fs::symlink(&victim, &planted).expect("plant");
    let again = rig.run(&ask(&path), &root);
    assert_eq!(
        rig.jobs.get(),
        1,
        "a symlink refuses the writer before rendering"
    );
    assert_eq!(
        std::fs::read(&victim).unwrap(),
        [PNG, b"keep me"].concat(),
        "the planted target is untouched"
    );
    assert!(planted.symlink_metadata().unwrap().file_type().is_symlink());
    assert!(again.contains("left unchanged"));
}

#[test]
fn the_cache_directory_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let d = TestDir::new("fontmode");
    Rig::new().run(&ask(&font(&d, "a.ttf")), &d.join("cache"));
    assert_eq!(
        std::fs::metadata(d.join("cache"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

#[test]
fn the_wire_lines_have_the_documented_shape() {
    let want = r##"{"c":"fontlook","id":4,"path":"/f/Inter-Bold.ttf","fg":"#c0caf5","bg":"#1a1b26","sizes":[33,22,18],"text":"The quick"}"##;
    let parsed = Ask::parse(want);
    assert_eq!(
        (
            parsed.id,
            parsed.fg.as_str(),
            parsed.sizes.clone(),
            parsed.text.as_str()
        ),
        (4, "#c0caf5", vec![33, 22, 18], "The quick")
    );
    let facts = Facts {
        family: Some("Inter".into()),
        style: Some("Bold".into()),
        glyphs: Some(2548),
    };
    let files = vec![
        PathBuf::from("/c/a-0.png"),
        PathBuf::from("/c/a-1.png"),
        PathBuf::from("/c/a-2.png"),
    ];
    assert_eq!(
        looked_line(4, &facts, &files),
        r#"{"t":"fontlooked","id":4,"family":"Inter","style":"Bold","glyphs":2548,"files":["/c/a-0.png","/c/a-1.png","/c/a-2.png"]}"#
    );
    assert_eq!(
        looked_line(5, &Facts::default(), &files),
        r#"{"t":"fontlooked","id":5,"files":["/c/a-0.png","/c/a-1.png","/c/a-2.png"]}"#
    );
    assert_eq!(
        failed_line(6, "no \"x\""),
        r#"{"t":"fontlooked","id":6,"err":"no \"x\""}"#
    );
}

// The production runner's jail with a stub in place of magick: the work folder is writable, the font is read only.
#[test]
fn the_jail_lets_the_stub_write_the_work_folder_and_not_the_font() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let d = TestDir::new("fontjail");
    let path = font(&d, "a.ttf");
    let root = d.dir("cache");
    let mut work = Work::new(&root, WORK_TAG).expect("work");
    let script = "import sys\nfor p in sys.argv[2:]:\n    open(p, 'wb').write(bytes([0x89, 0x50, 0x4e, 0x47, 13, 10, 26, 10]))\ntry:\n    open(sys.argv[1], 'ab')\n    sys.exit(3)\nexcept OSError:\n    pass\n";
    let drawn: Vec<String> = (0..SAMPLES)
        .map(|i| {
            work.dir
                .join(format!("{}.png", i))
                .to_string_lossy()
                .to_string()
        })
        .collect();
    let mut inner = vec![
        "/usr/bin/python3".to_string(),
        "-c".to_string(),
        script.to_string(),
        path.to_string_lossy().to_string(),
    ];
    inner.extend(drawn);
    let cancel = AtomicBool::new(false);
    let result = run_boxed_cancellable_capped("font", inner, &path, &mut work, &cancel);
    assert!(
        result.is_ok(),
        "the stub must write its folder and fail to write the font: {:?}",
        result.err().map(|e| e.msg)
    );
    assert!((0..SAMPLES).all(|i| is_png(&work.dir.join(format!("{}.png", i)))));
}

#[test]
fn foreign_and_newer_sample_documents_survive_read_and_first_render() {
    let d = TestDir::new("font-preserve");
    let rig = Rig::new();
    let path = font(&d, "Inter.ttf");
    let ask = ask(&path);
    let root = d.dir("cache");
    let key = key(&path, &path.metadata().unwrap(), &ask, &ask.text);
    let target = entry(&root, &key, 0);
    for original in [
        "{\"foreign\":true}",
        "{\"schemaVersion\":999}",
        "unparsable",
    ] {
        std::fs::write(&target, original).unwrap();
        assert!(cached(&root, &key).is_none());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
        assert!(rig.run(&ask, &root).contains("left unchanged"));
        prune(&root);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
    }
    assert_eq!(rig.jobs.get(), 0);
}

// S1A-9: a font with no Latin letters draws a sample its cmap covers, under its own cache key, never a blank specimen.
#[test]
fn a_font_with_no_latin_letters_draws_a_sample_it_covers() {
    let d = TestDir::new("font-georgian");
    let rig = Rig::new();
    let path = d.join("Georgian.ttf");
    std::fs::write(
        &path,
        crate::backend::fontinfo::sample::tests::font_with(&[(0x20, 0x40), (0x10D0, 0x10FF)]),
    )
    .unwrap();
    rig.run(&ask(&path), &d.join("cache"));
    let seen = rig.seen.borrow();
    let label = seen[0]
        .iter()
        .find(|a| a.starts_with("label:"))
        .expect("a label");
    assert!(
        label.starts_with("label:\u{10D0}\u{10D1}"),
        "the Georgian alphabet is drawn: {label}"
    );
}
