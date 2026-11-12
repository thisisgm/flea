use super::*;
use crate::backend::{convert, thumbargv, thumbspec::Spec};

fn png(path: &std::path::Path) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    assert!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "{path:?} is not PNG"
    );
}

fn red(path: &std::path::Path) {
    let args = [
        "magick".into(),
        "-read".into(),
        flea::bytename::path_text(path).into_owned(),
        "-format".into(),
        "%[pixel:p{0,0}]".into(),
        "info:".into(),
    ];
    let pixel = printtool::run(&args, &AtomicBool::new(false), printtool::TOOL_WAIT).unwrap();
    assert_eq!(
        pixel, "srgb(255,0,0)",
        "the selected source supplies the pixels"
    );
}

// A padded or framed output keeps white around the source, so only a red source can bring blue down to 0.
fn holds_red(path: &std::path::Path) {
    let args = [
        "magick".into(),
        "-read".into(),
        flea::bytename::path_text(path).into_owned(),
        "-format".into(),
        "%[fx:minima.b]".into(),
        "info:".into(),
    ];
    let blue = printtool::run(&args, &AtomicBool::new(false), printtool::TOOL_WAIT).unwrap();
    assert_eq!(
        blue, "0",
        "{path:?} holds the selected source, not a blue decoy"
    );
}

#[test]
fn magick_user_paths_are_literal_for_reads_and_writes() {
    if !ready(&["magick", "montage", "convert"]) {
        return;
    }
    let d = TestDir::new("magick-user-paths");
    for name in ["1", "%1"] {
        std::fs::create_dir(d.join(name)).unwrap();
        ppm(&d, &format!("{name}/{name}.ppm"), 4, 2, "0 0 255\n");
    }
    let original =
        b"P3\n4 2\n255\n255 0 0 255 0 0 255 0 0 255 0 0\n255 0 0 255 0 0 255 0 0 255 0 0\n";
    for name in [
        "50% off",
        "a%%b",
        "scan%d",
        "%[fx:1]",
        "%[filename:foo]",
        "png:photo",
        "-photo",
    ] {
        let folder = d.join(name);
        std::fs::create_dir(&folder).unwrap();
        let input = folder.join(format!("{name}.ppm"));
        std::fs::write(&input, original).unwrap();
        let run = |args: Vec<String>| {
            printtool::run(&args, &AtomicBool::new(false), printtool::TOOL_WAIT)
                .unwrap_or_else(|e| panic!("{name}: {args:?}: {}", e.message));
        };
        let output = |kind: &str| folder.join(format!("{name}-{kind}.png"));
        let converted = output("legacy");
        run(convert::argv(&input, &converted, true));
        png(&converted);
        red(&converted);
        let converted = output("batch");
        run(convertbatch::image_argv(
            std::slice::from_ref(&input),
            &converted,
            false,
        ));
        png(&converted);
        red(&converted);
        let size = printtool::run(
            &combine::identify_argv(std::slice::from_ref(&input)),
            &AtomicBool::new(false),
            printtool::TOOL_WAIT,
        )
        .unwrap_or_else(|e| panic!("{name}: size probe: {}", e.message));
        assert_eq!(
            size, "4 2\n",
            "{name}: the combine size probe reads the source"
        );
        for layout in [Layout::Row, Layout::Column, Layout::Grid] {
            let settings = Settings {
                layout,
                spacing: Spacing::Small,
                background: Background::White,
                columns: 2,
            };
            let scaled = output(&format!("{layout:?}-scaled"));
            run(combine::scale_argv(
                &input,
                &scaled,
                (100, 60),
                &settings,
                true,
            ));
            png(&scaled);
            holds_red(&scaled);
            let joined = output(&format!("{layout:?}-joined"));
            for args in
                combine::join_commands(&[scaled.clone(), scaled], &joined, (100, 60), &settings)
            {
                run(args);
            }
            png(&joined);
            holds_red(&joined);
        }
        for command in [
            vec!["magick"],
            vec!["magick", "--"],
            vec!["convert"],
            vec!["magick", "convert"],
            vec!["magick", "montage"],
        ] {
            let thumbnail = output(&command.join("-"));
            let flat = command.last() != Some(&"montage");
            let spec = Spec {
                exec: command
                    .into_iter()
                    .chain(["%i", "-thumbnail", "%s", "%o"])
                    .map(String::from)
                    .collect(),
            };
            let (_, args) = thumbargv::argv(&spec, &input, &thumbnail, 8).unwrap();
            run(args);
            png(&thumbnail);
            if flat {
                red(&thumbnail);
            } else {
                holds_red(&thumbnail);
            }
        }
        assert_eq!(std::fs::read(&input).unwrap(), original);
    }
}

#[test]
fn magick_percent_paths_keep_non_utf8_bytes() {
    if !ready(&["magick"]) {
        return;
    }
    use std::os::unix::ffi::OsStrExt;
    let d = TestDir::new("magick-raw-paths");
    let source = d
        .join("")
        .join(std::ffi::OsStr::from_bytes(b"source-%[fx:1]-\xff.ppm"));
    std::fs::write(&source, b"P3\n1 1\n255\n255 0 0\n").unwrap();
    let dest = d
        .join("")
        .join(std::ffi::OsStr::from_bytes(b"dest-%d-\xff.png"));
    let args = combine::scale_argv(
        &source,
        &dest,
        (1, 1),
        &Settings {
            layout: Layout::Row,
            spacing: Spacing::None,
            background: Background::White,
            columns: 2,
        },
        true,
    );
    printtool::run(&args, &AtomicBool::new(false), printtool::TOOL_WAIT).unwrap();
    png(&dest);
}

#[test]
fn magick_font_paths_and_intermediate_writes_remain_literal() {
    if !ready(&["magick"]) {
        return;
    }
    use crate::backend::fontinfo::render;
    use std::os::unix::ffi::OsStrExt;
    let d = TestDir::new("magick-font-paths");
    let font = d
        .join("")
        .join(std::ffi::OsStr::from_bytes(b"a%%-%[fx:1]-\xff.ttf"));
    std::fs::write(
        &font,
        include_bytes!("../../crates/flea-gui/tests/fonts/JetBrainsMono-Regular.ttf"),
    )
    .unwrap();
    let ask = render::Ask {
        id: 1,
        path: font.clone(),
        fg: "black".into(),
        bg: "white".into(),
        sizes: vec![20, 16],
        text: "Sample".into(),
    };
    let outputs = [d.join("a%%-%[fx:1].png"), d.join("b%%-%d.png")];
    let args = render::argv("magick", &font, &ask, "Sample", &outputs);
    let result = std::process::Command::new(&args[0])
        .args(flea::bytename::args(&args[1..]))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        result.stderr.is_empty(),
        "the intended font must render without fallback: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    for output in outputs {
        png(&output);
    }
}

#[test]
fn magick_relative_prefixes_and_expressions_remain_file_names() {
    if !ready(&["magick"]) {
        return;
    }
    let d = TestDir::new("magick-relative-paths");
    for name in ["png:photo", "-photo", "%[fx:1]", "a%%b"] {
        let source = format!("{name}.ppm");
        let output = format!("{name}.png");
        ppm(&d, &source, 4, 2, "255 0 0\n");
        let args = convert::argv(
            std::path::Path::new(&source),
            std::path::Path::new(&output),
            false,
        );
        let result = std::process::Command::new(&args[0])
            .args(flea::bytename::args(&args[1..]))
            .current_dir(d.join(""))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        png(&d.join(&output));
    }
}

// magick expands %d anywhere in an output path, so a folder named scan%d sent a convert or a combine into scan0.
#[test]
fn print_real_a_percent_folder_keeps_every_magick_output_where_it_was_asked() {
    if !ready(&["magick", "montage"]) {
        return;
    }
    let d = TestDir::new("print-real-percent");
    std::fs::create_dir_all(d.join("scan%d")).unwrap();
    std::fs::create_dir_all(d.join("scan0")).unwrap();
    let a = ppm(&d, "a.ppm", 4, 2, "255 0 0\n");
    let run = |argv: Vec<String>| {
        printtool::run(&argv, &AtomicBool::new(false), printtool::TOOL_WAIT)
            .unwrap_or_else(|e| panic!("{argv:?}: {}", e.message));
    };
    let converted = d.join("scan%d/c%d.png");
    run(convertbatch::image_argv(
        std::slice::from_ref(&a),
        &converted,
        false,
    ));
    let settings = Settings {
        layout: Layout::Grid,
        spacing: Spacing::Small,
        background: Background::White,
        columns: 2,
    };
    let scaled = d.join("scan%d/s%d.png");
    run(combine::scale_argv(&a, &scaled, (100, 60), &settings, true));
    let joined = d.join("scan%d/j%d.png");
    for command in combine::join_commands(
        &[scaled.clone(), scaled.clone()],
        &joined,
        (100, 60),
        &settings,
    ) {
        run(command);
    }
    for path in [&converted, &scaled, &joined] {
        assert!(path.is_file(), "{path:?} was not written");
    }
    assert_eq!(
        std::fs::read_dir(d.join("scan0")).unwrap().count(),
        0,
        "nothing lands in scan0"
    );
}
