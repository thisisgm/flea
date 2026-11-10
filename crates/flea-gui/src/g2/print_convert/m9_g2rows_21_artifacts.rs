use super::{
    check,
    model::{Action, Card, Focus, Model},
    seam::OutputError,
};
use flea::ui_logic_print_convert::{
    composition,
    output::Kind,
    selection::{Selection, Source},
};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

struct Sandbox(PathBuf, PathBuf);
impl Drop for Sandbox {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && !self.0.as_os_str().is_empty());
        assert_eq!(self.0.parent(), Some(self.1.as_path()));
        assert!(self.0.join(".flea-test-sandbox").is_file());
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn run(arguments: &[String]) -> Vec<u8> {
    let output = Command::new(&arguments[0])
        .args(&arguments[1..])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn encode(sources: &[PathBuf], output: &Path, settings: &composition::Settings) {
    let shared = composition::shared_size(settings.layout, &[(10, 10), (10, 10)]).unwrap();
    let mut scaled = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let destination = output.with_extension(format!("scaled-{index}.png"));
        run(&composition::scale_argv(
            source,
            &destination,
            shared,
            settings,
            index == 0,
        ));
        scaled.push(destination);
    }
    for command in composition::join_commands(&scaled, output, shared, settings) {
        run(&command);
    }
}

fn pixels(path: &Path) -> Vec<u8> {
    run(&[
        "magick".into(),
        path.to_string_lossy().into_owned(),
        "-depth".into(),
        "8".into(),
        "rgb:-".into(),
    ])
}

pub(super) fn check_artifacts() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/m9-g2rows-21");
    std::fs::create_dir_all(&parent).unwrap();
    let parent = parent.canonicalize().unwrap();
    let made = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("artifacts.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim_end());
    assert!(root.is_absolute() && root.parent() == Some(parent.as_path()));
    std::fs::write(root.join(".flea-test-sandbox"), b"output artifacts").unwrap();
    let _sandbox = Sandbox(root.clone(), parent);
    let catalog = crate::test_catalogs::english();
    let mut files = Vec::new();
    for (name, color) in [("z.png", "red"), ("a.png", "blue")] {
        let path = root.join(name);
        run(&[
            "magick".into(),
            "-size".into(),
            "10x10".into(),
            format!("xc:{color}"),
            path.to_string_lossy().into_owned(),
        ]);
        files.push(Source {
            path: path.to_string_lossy().into_owned(),
            kind: Kind::Image,
        });
    }
    let selection = Selection {
        generation: 31,
        menu: 0,
        files,
    };
    let mut model = Model::default();
    assert!(model.open(Action::Combine, selection.clone(), &catalog));
    model.move_image(0, 1, &catalog);
    for (layout, dimensions) in [(0, "20 10"), (1, "10 20"), (2, "20 10")] {
        model.setting(Focus::Layout, layout, &catalog);
        let Card::Combine(card) = model.card.as_ref().unwrap() else {
            panic!()
        };
        let output = card.output(model.selection.as_ref().unwrap()).unwrap();
        assert_eq!(
            check::paths(vec![output.clone()]).unwrap().as_slice(),
            std::slice::from_ref(&output)
        );
        let sources = model
            .selection
            .as_ref()
            .unwrap()
            .files
            .iter()
            .map(|f| PathBuf::from(&f.path))
            .collect::<Vec<_>>();
        encode(&sources, Path::new(&output), &card.settings);
        let control_path = root.join(format!("unreordered-{layout}.png"));
        let original_sources = selection
            .files
            .iter()
            .map(|file| PathBuf::from(&file.path))
            .collect::<Vec<_>>();
        encode(&original_sources, &control_path, &card.settings);
        let control = pixels(&control_path);
        assert_eq!(
            &control[..3],
            &[255, 0, 0],
            "control starts in captured order"
        );
        assert_eq!(&control[control.len() - 3..], &[0, 0, 255]);
        let actual = run(&[
            "magick".into(),
            "identify".into(),
            "-format".into(),
            "%w %h".into(),
            output.clone(),
        ]);
        assert_eq!(String::from_utf8(actual).unwrap(), dimensions);
        let pixels = pixels(Path::new(&output));
        assert_eq!(
            &pixels[..3],
            &[0, 0, 255],
            "the first tile follows the reordered selection"
        );
        assert_eq!(
            &pixels[pixels.len() - 3..],
            &[255, 0, 0],
            "the last tile is the earlier first image"
        );
        assert_ne!(
            pixels, control,
            "the separately encoded unreordered control differs"
        );
        eprintln!(
            "output artifact {:?}: {dimensions}, first RGB {:?}, last RGB {:?}",
            card.settings.layout,
            &pixels[..3],
            &pixels[pixels.len() - 3..]
        );
        assert!(matches!(
            check::paths(vec![output.clone()]),
            Err(OutputError::Exists(_))
        ));
        let copy = root.join(format!("proof-{layout}.png"));
        assert!(
            Path::new(&output).is_absolute() && Path::new(&output).parent() == Some(root.as_path())
        );
        std::fs::rename(output, copy).unwrap();
    }
    assert!(model.close());
    let pdf = root.join("report.pdf");
    let pdf_selection = Selection {
        generation: 32,
        menu: 0,
        files: vec![Source {
            path: pdf.to_string_lossy().into_owned(),
            kind: Kind::Pdf,
        }],
    };
    assert!(model.open(Action::Convert, pdf_selection, &catalog));
    let Card::Convert(card) = model.card.as_ref().unwrap() else {
        panic!()
    };
    let ticket = card.count_pending.unwrap();
    model.document_pages(ticket, Ok(3), &catalog);
    let Card::Convert(card) = model.card.as_ref().unwrap() else {
        panic!()
    };
    assert_eq!(card.outputs.len(), 3);
    assert!(card
        .outputs
        .iter()
        .enumerate()
        .all(|(index, path)| path.ends_with(&format!("report (page {}).png", index + 1))));
    let first = card.outputs[0].clone();
    let original = std::fs::read(root.join("z.png")).unwrap();
    std::fs::write(&first, &original).unwrap();
    let resolved = check::converted(card.outputs.clone(), true).unwrap();
    assert!(resolved[0].ends_with("report (page 1) (converted).png"));
    assert_eq!(std::fs::read(&first).unwrap(), original);
    std::fs::write(&resolved[0], b"existing output").unwrap();
    let stepped = check::converted(card.outputs.clone(), true).unwrap();
    assert!(
        stepped[0].ends_with("report (page 1) (converted) copy.png"),
        "{stepped:?}"
    );
    assert_eq!(std::fs::read(&resolved[0]).unwrap(), b"existing output");
}
