use super::*;
use crate::backend::menu_actions::Snapshot;
use crate::backend::testdir::TestDir;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn app(id: &str, default: bool) -> Application {
    Application {
        id: id.into(),
        label: id.into(),
        path: PathBuf::new(),
        icon: String::new(),
        default,
        exec: String::new(),
    }
}

fn ids(apps: &[Application]) -> Vec<(&str, bool)> {
    apps.iter()
        .map(|app| (app.id.as_str(), app.default))
        .collect()
}

#[test]
fn several_types_offer_only_the_applications_every_type_names() {
    let png = || {
        vec![
            app("imv.desktop", true),
            app("gimp.desktop", false),
            app("chromium.desktop", false),
        ]
    };
    let jpeg = vec![
        app("gimp.desktop", true),
        app("chromium.desktop", false),
        app("imv.desktop", false),
    ];
    let webp = vec![
        app("chromium.desktop", false),
        app("imv.desktop", false),
        app("gimp.desktop", false),
    ];
    assert_eq!(
        ids(&intersect(vec![png(), png()])),
        [
            ("imv.desktop", true),
            ("gimp.desktop", false),
            ("chromium.desktop", false)
        ],
        "one type twice keeps its own registry and its default first"
    );
    assert_eq!(
        ids(&intersect(vec![png(), jpeg])),
        [
            ("imv.desktop", false),
            ("gimp.desktop", false),
            ("chromium.desktop", false)
        ],
        "a default for only some of the types is no default for the selection"
    );
    let pdf = vec![app("zathura.desktop", true), app("chromium.desktop", false)];
    assert_eq!(
        ids(&intersect(vec![png(), webp, pdf])),
        [("chromium.desktop", false)]
    );
    assert!(
        intersect(vec![png(), vec![app("zathura.desktop", true)]]).is_empty(),
        "no shared application leaves the flyout empty"
    );
    assert!(intersect(Vec::new()).is_empty());
}

#[test]
fn gio_info_names_one_type_per_file_or_refuses() {
    let info = "uri: file:///p/a.png\nlocal path: /p/a.png\nattributes:\n  standard::content-type: image/png\nuri: file:///p/b.jpg\nlocal path: /p/b.jpg\nattributes:\n  standard::content-type: image/jpeg\n";
    assert_eq!(content_types(info, 2).unwrap(), ["image/png", "image/jpeg"]);
    assert_eq!(
        content_types(info, 3).unwrap_err(),
        "The selected item's file type is unknown."
    );
}

#[test]
fn a_list_entry_launches_once_and_a_file_entry_once_per_file() {
    let (a, b, c) = (
        Path::new("/p/a.png"),
        Path::new("/p/b png.png"),
        Path::new("/p/c.png"),
    );
    let desktop = Path::new("/usr/share/applications/viewer.desktop");
    let argv = |exec: &str| {
        groups(exec, &[a, b, c])
            .iter()
            .map(|group| launch_args(desktop, group))
            .collect::<Vec<_>>()
    };
    let all: Vec<OsString> = [
        "launch",
        "/usr/share/applications/viewer.desktop",
        "/p/a.png",
        "/p/b png.png",
        "/p/c.png",
    ]
    .map(OsString::from)
    .to_vec();
    assert_eq!(argv("imv %F").as_slice(), std::slice::from_ref(&all));
    assert_eq!(argv("chromium %U").as_slice(), std::slice::from_ref(&all));
    let one = |path: &str| {
        ["launch", "/usr/share/applications/viewer.desktop", path]
            .map(OsString::from)
            .to_vec()
    };
    let each = [one("/p/a.png"), one("/p/b png.png"), one("/p/c.png")];
    assert_eq!(argv("gimp %f"), each);
    assert_eq!(argv("browser %u"), each);
    assert_eq!(
        argv("printf 100%%F %f"),
        each,
        "a literal percent before F is no list code"
    );
    assert_eq!(argv("viewer"), each);
    assert_eq!(named(&[a]), "/p/a.png");
    assert_eq!(named(&[a, b, c]), "3 items");
}

// Sample /proc/PID/cmdline fields: "/bin/sh\0/tmp/flea-test-.../record\0..."; recorder children retain paths inside this test's marked root.
fn recorders_pending(dir: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let root = dir.parent().unwrap();
    std::fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().as_bytes().iter().all(u8::is_ascii_digit))
        .filter_map(|entry| std::fs::read(entry.path().join("cmdline")).ok())
        .any(|args| {
            args.split(|byte| *byte == 0)
                .any(|arg| Path::new(std::ffi::OsStr::from_bytes(arg)).starts_with(root))
        })
}

// GIO has returned after exec; wait for the fixture's remaining recorder children before counting their complete writes.
fn recorded(dir: &Path, runs: usize) -> Vec<Vec<String>> {
    let started = Instant::now();
    loop {
        let pending = recorders_pending(dir);
        let mut found: Vec<Vec<String>> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("ran."))
            .filter_map(|entry| {
                std::fs::read_to_string(entry.path())
                    .ok()
                    .filter(|text| text.ends_with("end\n"))
            })
            .map(|text| {
                text.lines()
                    .filter(|line| *line != "end")
                    .map(str::to_string)
                    .collect()
            })
            .collect();
        if found.len() >= runs && !pending {
            found.sort();
            return found;
        }
        assert!(
            started.elapsed() <= Duration::from_secs(10),
            "recorder children or complete writes did not finish before the deadline"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn gio_runs_a_list_entry_once_with_every_file_and_a_file_entry_once_per_file() {
    for (code, want) in [("%F", 1), ("%f", 2)] {
        let sandbox = TestDir::new("menu-many-launch");
        let out = sandbox.dir("out");
        let recorder = sandbox.script("record", &format!("#!/bin/sh\nprintf '%s\\n' \"$@\" end > {}/ran.$$.tmp && mv {}/ran.$$.tmp {}/ran.$$\n", out.display(), out.display(), out.display()));
        let desktop = sandbox.file(
            "viewer.desktop",
            &format!(
                "[Desktop Entry]\nType=Application\nName=Viewer\nExec={} {code}\n",
                recorder.display()
            ),
        );
        let (a, b) = (sandbox.file("a.png", "a"), sandbox.file("b.png", "b"));
        let checked = std::cell::RefCell::new(Vec::new());
        launch(
            &Registry::default(),
            &desktop,
            &[&a, &b],
            |range| {
                checked.borrow_mut().push(range);
                Ok(())
            },
            &Cancellation::default(),
        )
        .unwrap();
        let expected = if want == 1 {
            std::iter::once(0..2).collect()
        } else {
            vec![0..1, 1..2]
        };
        assert_eq!(
            *checked.borrow(),
            expected,
            "each selected identity is checked once, immediately before its launch group"
        );
        let (a, b) = (a.display().to_string(), b.display().to_string());
        let runs = recorded(&out, want);
        if want == 1 {
            assert_eq!(runs, [vec![a, b]], "{code}");
        } else {
            assert_eq!(runs, [vec![a], vec![b]], "{code}");
        }
    }
}

#[test]
fn a_changed_selection_stops_the_launch_before_gio_runs() {
    let sandbox = TestDir::new("menu-many-changed");
    let not_executable = sandbox.file("not-executable", "plain text");
    let desktop = sandbox.file(
        "viewer.desktop",
        &format!(
            "[Desktop Entry]\nType=Application\nName=Viewer\nExec={} %F\n",
            not_executable.display()
        ),
    );
    let a = sandbox.file("a.png", "a");
    let refused = launch(
        &Registry::default(),
        &desktop,
        &[&a, &a],
        |_| Err("Selected items changed; reopen the menu.".into()),
        &Cancellation::default(),
    );
    assert_eq!(
        refused.unwrap_err(),
        "Selected items changed; reopen the menu."
    );
}

#[test]
fn a_menu_over_several_files_answers_its_applications_instead_of_refusing() {
    let sandbox = TestDir::new("menu-many-applications");
    let paths = ["a.txt", "b.txt", "c.txt"]
        .map(|name| sandbox.file(name, "text\n").to_string_lossy().to_string())
        .to_vec();
    let mut snapshot = Snapshot::default();
    assert!(snapshot
        .handle(r#"{"op":"snapshot","id":5}"#, paths)
        .contains(r#""ok":true"#));
    let reply = snapshot.handle(r#"{"op":"applications","id":5}"#, vec![]);
    assert!(
        reply.contains(r#""ok":true"#) && reply.contains(r#""applications":["#),
        "{reply}"
    );
    assert!(!reply.contains("requires one selected item"), "{reply}");
}
