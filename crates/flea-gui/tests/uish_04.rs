use flea::ipc::wire::{Reply, Request};
use flea_gui::{
    ipc::{native_window::NativeWindowAdapter, Surface},
    list::{pane::Held, ListElement, ListState, Typeface},
    palette::{ParsedTheme, Theme},
    presentation::Fonts,
};
use gpui::{div, prelude::*, AppContext, Context, Render, TestAppContext, Window};
use std::{cell::RefCell, rc::Rc};

struct Probe {
    list: Rc<RefCell<ListState>>,
    held: Rc<Held>,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(ListElement::new(
            self.held.clone(),
            0.,
            0,
            self.list.clone(),
        ))
    }
}

fn call(list: &ListState, window: &Window, theme: Theme, parsed: ParsedTheme, verb: &str) -> Reply {
    let adapter = NativeWindowAdapter {
        list,
        theme,
        parsed,
        viewport: window.viewport_size(),
        corner: 0.,
    };
    Surface {
        path: "/observer-control",
        total: 1,
        cursor: 0,
        selected: &[],
    }
    .dispatch_to(
        Request::Call {
            target: "flea".into(),
            function: verb.into(),
            arguments: vec![],
        },
        &[&adapter],
    )
    .reply
}

#[gpui::test]
fn uish_04_real_window(cx: &mut TestAppContext) {
    let list = Rc::new(RefCell::new(ListState::new(Typeface::omarchy_default())));
    let handle = cx.add_window(|_, _| Probe {
        list: list.clone(),
        held: Rc::new(Held {
            listing: 1,
            total: 1,
            start: 0,
            rows: vec![flea::protocol::RowFacts {
                name: "control.txt".into(),
                ..Default::default()
            }],
        }),
    });
    let mut palettes = Vec::new();
    let mut sizes = Vec::new();
    let mut token_replies = Vec::new();
    for (base, text) in [
        (
            12,
            "background='#101315'\nforeground='#c1c497'\naccent='#e5a55b'",
        ),
        (
            14,
            "background='#101315'\nforeground='#c1c497'\naccent='#e5a55b'",
        ),
        (
            16,
            "background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'",
        ),
    ] {
        let (theme, parsed) = Theme::from_text_observed(text);
        let face =
            Fonts::from_sources(&format!("[font]\nbase-size={base}"), "", "monospace".into())
                .typeface(&flea::jsondoc::Json::Null);
        list.borrow_mut().set_typeface(face);
        assert_eq!(
            handle
                .update(cx, |_, window, _| call(
                    &list.borrow(),
                    window,
                    theme,
                    parsed,
                    "tokens"
                ))
                .unwrap(),
            Reply::error(3, "FLEA_IPC_UNAVAILABLE flea.tokens: font not measured")
        );
        handle
            .update(cx, |_, _, cx| {
                cx.set_global(theme);
                cx.set_global(parsed);
            })
            .unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let state = list.borrow();
            assert!(state.drawn.frames > 0);
            let before = (state.drawn.frames, state.rows.misses, state.palette_reads);
            let palette = call(&state, window, theme, parsed, "palette");
            assert_eq!(palette.code, 0, "{palette:?}");
            let fields: Vec<_> = palette.stdout.split_whitespace().collect();
            assert_eq!(fields.len(), 10);
            let painted = state.drawn.palette.unwrap();
            assert_eq!(fields[2], flea_gui::ipc::rows::colour(painted.foreground));
            assert_eq!(fields[4], flea_gui::ipc::rows::colour(painted.accent));
            assert_eq!(fields[6], flea_gui::ipc::rows::colour(painted.symlink));
            let size = call(&state, window, theme, parsed, "bodyPx");
            assert_eq!(size, Reply::value(base.to_string()));
            let tokens = call(&state, window, theme, parsed, "tokens");
            assert_eq!(tokens.code, 0, "{tokens:?}");
            let pairs: Vec<_> = tokens
                .stdout
                .trim_end()
                .lines()
                .map(|line| line.split_once('=').unwrap())
                .collect();
            let oracle = include_str!("../../../ui/Theme.qml")
                .split("function tokens() {")
                .nth(1)
                .unwrap()
                .split("};")
                .next()
                .unwrap();
            let keys: Vec<_> = oracle
                .lines()
                .filter_map(|line| line.split_once(':').map(|(key, _)| key.trim()))
                .collect();
            assert_eq!(pairs.len(), 34);
            assert_eq!(pairs.iter().map(|(key, _)| *key).collect::<Vec<_>>(), keys);
            let value = |key| pairs.iter().find(|(name, _)| *name == key).unwrap().1;
            assert_eq!(value("baseSize"), base.to_string());
            assert_eq!(value("body"), base.to_string());
            let expected = match base {
                12 => ["11", "10", "6", "32", "20"],
                14 => ["13", "12", "7", "37", "23"],
                16 => ["15", "13", "8", "43", "27"],
                _ => unreachable!(),
            };
            for (key, expected) in [
                "bodySmall",
                "caption",
                "rowPaddingY",
                "rowHeight",
                "iconSize",
            ]
            .into_iter()
            .zip(expected)
            {
                assert_eq!(value(key), expected, "{base} {key}");
            }
            let mark = value("markSize").parse::<f32>().unwrap();
            let painted_mark = flea_gui::list::Metrics::for_typeface(&state.typeface).mark;
            assert_eq!(
                mark, painted_mark,
                "the observer reports the painted f32 metric"
            );
            let qml_mark = value("bodySmall").parse::<f64>().unwrap() * 1.45;
            assert!((f64::from(mark) - qml_mark).abs() < 0.00001);
            assert_eq!(
                call(&state, window, theme, parsed, "wheelLines"),
                Reply::value("3")
            );
            assert_eq!(
                before,
                (state.drawn.frames, state.rows.misses, state.palette_reads)
            );
            palettes.push(palette.stdout);
            sizes.push(size.stdout);
            token_replies.push(tokens.stdout);
        })
        .unwrap();
    }
    assert_ne!(palettes[0], palettes[2]);
    assert_ne!(sizes[0], sizes[1]);
    assert_ne!(token_replies[0], token_replies[1]);
    assert_ne!(token_replies[1], token_replies[2]);
}

#[gpui::test]
fn uish_04_stale_or_refused(cx: &mut TestAppContext) {
    let list = ListState::new(Typeface::omarchy_default());
    let window = cx.add_empty_window();
    window.update(|window, _| {
        for text in ["", "foreground='invalid'"] {
            let (theme, parsed) = Theme::from_text_observed(text);
            assert_eq!(
                call(&list, window, theme, parsed, "themeLoaded"),
                Reply::value("false")
            );
            assert_eq!(
                call(&list, window, theme, parsed, "palette"),
                Reply::error(3, "FLEA_IPC_UNAVAILABLE flea.palette: theme not loaded")
            );
            assert_eq!(
                call(&list, window, theme, parsed, "tokens"),
                Reply::error(3, "FLEA_IPC_UNAVAILABLE flea.tokens: theme not loaded")
            );
        }
        let (theme, parsed) = Theme::from_text_observed("foreground='#123456'");
        assert_eq!(
            call(&list, window, theme, parsed, "tokens"),
            Reply::error(3, "FLEA_IPC_UNAVAILABLE flea.tokens: font not measured")
        );
        assert_eq!(
            call(&list, window, theme, parsed, "wheelLines"),
            Reply::value("3")
        );
        assert_eq!(
            call(&list, window, theme, parsed, "glass"),
            Reply::error(3, "FLEA_IPC_NOT_PORTED flea.glass")
        );
    });
}

// Removes the harness root on every exit, pass or fail: whole once marked, else only while still empty.
struct Cleanup(std::path::PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = if self.0.join(".flea-test-sandbox").is_file() {
            std::fs::remove_dir_all(&self.0)
        } else {
            std::fs::remove_dir(&self.0)
        };
    }
}

#[test]
fn uish_04_live_theme_harness_controls() {
    use std::process::{Command, Stdio};
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let parent = repo.join(".superpowers/m9-uish-04");
    std::fs::create_dir_all(&parent).unwrap();
    let output = Command::new("/usr/bin/mktemp")
        .args(["-d"])
        .arg(parent.join("harness.XXXXXXXX"))
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.parent() == Some(parent.as_path()));
    let _cleanup = Cleanup(root.clone());
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(".flea-test-sandbox"))
        .unwrap();
    let output = Command::new("/usr/bin/setsid")
        .args(["-w", "/bin/bash"])
        .arg(repo.join("tests/ui-uish-common-observers.sh"))
        .arg("--controls")
        .arg(&root)
        .current_dir(&repo)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("UISH_04 HARNESS PASS 6 controls"));
    print!("{}", String::from_utf8_lossy(&output.stdout));
}
