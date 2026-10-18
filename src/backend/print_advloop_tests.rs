use super::*;

#[test]
fn adv_e_r1_print_reports_every_file_after_a_middle_refusal() {
    let mut rig = Rig::new(true);
    let first = rig.dir.file("first.pdf", "%PDF first");
    let refused = rig.dir.file("refuse.pdf", "%PDF middle");
    let last = rig.dir.file("last.pdf", "%PDF last");
    let command = format!(r#"{{"c":"print","requestId":71,"paths":["{}","{}","{}"],"printer":"Brother"}}"#, first.display(), refused.display(), last.display());
    let replies = rig.ask(&command, "printdone");
    let results: Vec<_> = replies.iter().filter(|line| field_str(line, "t").as_deref() == Some("printed")).collect();
    assert_eq!(results.len(), 3, "each selected file needs a result: {replies:?}");
    for (line, path) in results.iter().zip([&first, &refused, &last]) {
        assert_eq!(field_str(line, "path").as_deref(), path.to_str());
    }
    assert_eq!(results.iter().map(|line| field_bool(line, "ok")).collect::<Vec<_>>(), [true, false, true]);
    assert_eq!(field_str(results[1], "message").as_deref(), Some("refuse.pdf was not printed: printer is paused."));
    let done = replies.last().unwrap();
    assert!(!field_bool(done, "ok"));
    assert_eq!(field_usize(done, "sent"), Some(2));
    assert!(field_str(done, "err").unwrap().contains("printer is paused"));
    let calls = rig.calls();
    let lp: Vec<_> = calls.lines().filter(|line| line.starts_with("lp\0")).collect();
    assert_eq!(lp.len(), 3);
    assert!(lp[2].contains(last.to_str().unwrap()));
}

#[test]
fn adv_e_r1_three_frame_tiff_keeps_one_progress_total() {
    let program = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("magick")).find(|path| path.is_file());
    let Some(program) = program else {
        writeln!(std::io::stderr(), "SKIP three-frame progress: ImageMagick is not installed").ok();
        return;
    };
    let mut rig = Rig::new(true);
    let source = rig.dir.join("three.tiff");
    let mut argv = vec![program.display().to_string()];
    for (name, pixel) in [("red.ppm", "255 0 0"), ("green.ppm", "0 255 0"), ("blue.ppm", "0 0 255")] {
        let frame = rig.dir.file(name, &format!("P3\n1 1\n255\n{pixel}\n"));
        argv.extend(["--".into(), frame.display().to_string()]);
    }
    rig.dir.assert_contains(&source);
    argv.push(source.display().to_string());
    printtool::run(&argv, &AtomicBool::new(false), printtool::TOOL_WAIT).unwrap();
    let frames = printtool::run(&[program.display().to_string(), "--".into(), source.display().to_string(), "-format".into(), "%n\n".into(), "info:".into()],
        &AtomicBool::new(false), printtool::TOOL_WAIT).unwrap();
    assert_eq!(frames.lines().count(), 3, "the fixture is a real three-frame TIFF");
    rig.dir.file("real-magick", &format!("{}\n", program.display()));
    let replies = rig.ask(&format!(r#"{{"c":"convertbatch","requestId":72,"path":"{}","format":"png"}}"#, source.display()), "convertdone");
    let done = replies.last().unwrap();
    assert!(field_bool(done, "ok"), "{replies:?}");
    let outputs = crate::json::field_str_array(done, "paths");
    assert_eq!(outputs.len(), 3, "every TIFF frame is published");
    assert!(outputs.iter().all(|path| std::path::Path::new(path).is_file()));
    let progress: Vec<_> = replies.iter().filter(|line| field_str(line, "t").as_deref() == Some("convertprogress"))
        .map(|line| (field_usize(line, "done").unwrap(), field_usize(line, "total").unwrap())).collect();
    println!("three-frame TIFF progress (done,total): {progress:?}");
    assert!(progress.len() >= 3, "conversion and publication both report progress");
    let total = progress[0].1;
    assert!(total > 0 && progress.iter().all(|(done, count)| *count == total && *done <= total), "one fixed total: {progress:?}");
    assert!(progress.windows(2).all(|pair| pair[0].0 <= pair[1].0), "progress never regresses");
    assert_eq!(progress.last().unwrap().0, total, "successful publication completes the job");
}
