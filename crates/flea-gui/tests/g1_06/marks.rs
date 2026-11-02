use flea::protocol::RowFacts;
use flea_gui::row::marks::{Language, Mark, EXTS, FILES};
use std::path::PathBuf;

// The shipped QML's own tables, read from the tree so a drift on either side fails here.
fn oracle(file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ui/js")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

// The "key": "value" pairs inside one `var NAME = { ... }` block.
fn object(text: &str, name: &str) -> Vec<(String, String)> {
    let start = text
        .find(&format!("var {name} = {{"))
        .unwrap_or_else(|| panic!("no {name} table"));
    let body = &text[start..start + text[start..].find("\n}").unwrap()];
    let quoted: Vec<&str> = body.split('"').skip(1).step_by(2).collect();
    quoted
        .chunks(2)
        .map(|pair| (pair[0].to_owned(), pair[1].to_owned()))
        .collect()
}

fn array(text: &str, name: &str) -> Vec<String> {
    let start = text
        .find(&format!("var {name} = ["))
        .unwrap_or_else(|| panic!("no {name} list"));
    let body = &text[start..start + text[start..].find(']').unwrap()];
    body.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

#[test]
fn language_tables_match_lang_marks_js_entry_for_entry() {
    let js = oracle("LangMarks.js");
    let pairs = |table: &[(&str, &str)]| {
        table
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect::<Vec<_>>()
    };
    assert_eq!(pairs(&EXTS), object(&js, "EXTS"));
    assert_eq!(pairs(&FILES), object(&js, "FILES"));
    assert_eq!(Language::slugs().collect::<Vec<_>>(), array(&js, "KEYS"));
    let paths = object(&oracle("LangMarkPaths.js"), "PATHS");
    assert_eq!(paths.len(), Language::slugs().count());
    for (slug, d) in paths {
        let language = Language::for_slug(&slug).unwrap_or_else(|| panic!("no logo for {slug}"));
        let (key, svg) = Mark::Language(language).svg();
        assert_eq!(key.as_ref(), format!("flea-lang:{slug}"));
        let svg = std::str::from_utf8(svg).unwrap();
        assert!(
            svg.contains(&format!("d=\"{d}\"")),
            "the {slug} logo is not LangMarkPaths.js's"
        );
        assert!(
            svg.contains("d=\"M4 7V2h10l6 6v14H17 M14 2v6h6\""),
            "the {slug} mark lost its page"
        );
    }
}

#[test]
fn a_language_is_found_by_basename_then_last_extension_case_blind() {
    let slug = |name: &str| Language::for_file(name).map(Language::slug);
    assert_eq!(slug("main.rs"), Some("rust"));
    assert_eq!(slug("Dockerfile"), Some("docker"));
    assert_eq!(slug("CMakeLists.txt"), Some("cmake"));
    assert_eq!(slug(".gitignore"), Some("git"));
    assert_eq!(slug("deep/path/App.TSX"), Some("react"));
    assert_eq!(slug("archive.tar.json"), Some("json"));
    assert_eq!(slug("notes.txt"), None);
    assert_eq!(slug(".rs"), None, "a leading dot is not an extension");
    let file = |name: &str, mode: u32, dir: bool| RowFacts {
        name: name.into(),
        mode,
        dir,
        icon: "text-x-script".into(),
        ..RowFacts::default()
    };
    assert!(matches!(
        Mark::for_facts(&file("main.rs", 0o100644, false), true),
        Mark::Language(_)
    ));
    assert_eq!(
        Mark::for_facts(&file("main.rs", 0o120777, false), true),
        Mark::Symlink,
        "a link never wears a logo"
    );
    assert_eq!(
        Mark::for_facts(&file("src.rs", 0o040755, true), true),
        Mark::Code,
        "a folder never wears a logo"
    );
    assert_eq!(
        Mark::for_facts(&file("main.rs", 0o100644, false), false),
        Mark::Code
    );
}

#[test]
fn glyph_names_follow_icons_js() {
    let js = oracle("Icons.js");
    let glyphs = object(&js, "GLYPHS");
    assert!(glyphs.len() >= 17, "the GLYPHS table did not parse");
    for (icon, glyph) in glyphs {
        assert_eq!(Mark::for_row(&icon, 0o100644).name(), glyph, "icon {icon}");
        assert_eq!(
            Mark::for_row(&icon, 0o120777).name(),
            "symlink",
            "a link to {icon} keeps the link mark"
        );
    }
    assert_eq!(
        Mark::for_row("application-x-unknown", 0o100644).name(),
        "file"
    );
}

#[test]
fn clipboard_bounds_align_in_the_reserved_strip() {
    use super::harness::{held, open, real_cx, row};
    use flea_gui::list::RowStates;
    use flea_gui::list::{RowState, Shown};
    use flea_gui::row::{cache::Prepared, marks::ClipMark};
    use gpui::{point, px, size, Bounds, GlyphRun, Hsla, TextRun};
    use std::rc::Rc;

    let names = [
        "a.txt".to_owned(),
        "very-long-name-".repeat(12),
        "文書-e\u{301}.txt".into(),
    ];
    let mut cx = real_cx();
    let mut saw_bold_advance = false;
    for width in [260., 900.] {
        let pane = open(
            &mut cx,
            held(
                names
                    .iter()
                    .map(|name| row(name, "text-plain", 26, false))
                    .collect(),
                1,
            ),
            width,
            5.,
        );
        let mut copied = Vec::new();
        for moving in [false, true] {
            {
                let mut state = pane.list.borrow_mut();
                state.view.clip.scope("/w");
                state.view.clip.set(
                    &names
                        .iter()
                        .map(|name| format!("/w/{name}"))
                        .collect::<Vec<_>>(),
                    moving,
                );
                state.view.set_query("a");
            }
            for rung in 0..4 {
                pane.update(&mut cx, |view| {
                    view.states = RowStates {
                        listing: 1,
                        cursor: (rung == 2).then_some(0),
                        hovered: (rung == 1).then_some(0),
                        selected: (rung == 3).then(|| Rc::from([0, 1, 2])),
                        ..RowStates::default()
                    }
                });
                pane.draw(&mut cx);
                let state = pane.list.borrow();
                let metrics = state.drawn.metrics;
                let layout = state.drawn.layout;
                for shown in state.visible() {
                    let prepared = shown.prepared.as_ref().unwrap();
                    let name = Bounds::new(
                        point(px(layout.name.x), px(0.)),
                        size(px(layout.name.width), px(metrics.row)),
                    );
                    let bounds = shown.clip_bounds(name, &metrics).unwrap();
                    let run = prepared
                        .bold
                        .get()
                        .filter(|_| shown.state.cursor)
                        .unwrap_or(&prepared.name);
                    saw_bold_advance |= shown.state.cursor
                        && (f32::from(run.line().width - prepared.name.line().width)).abs() > 0.01;
                    let drawn = f32::from(run.line().width)
                        .min((layout.name.width - metrics.gap - metrics.clip).max(0.));
                    assert!(
                        (f32::from(bounds.left()) - (layout.name.x + drawn + metrics.gap)).abs()
                            < 0.001,
                        "the clipboard mark follows the drawn name by one gap"
                    );
                    assert!(bounds.right() <= name.right());
                    assert_eq!(bounds.size, size(px(12.), px(12.)));
                    if moving {
                        assert_eq!(bounds, copied[rung * names.len() + shown.index]);
                    } else {
                        copied.push(bounds);
                    }
                }
            }
        }
        let misses = pane.list.borrow().rows.misses;
        pane.draw(&mut cx);
        assert_eq!(pane.list.borrow().rows.misses, misses);
        assert!(pane.list.borrow().rows.peak <= 15);
        // Unequal cached advances discriminate cursor-run selection even when the installed mono face gives bold equal widths.
        pane.with_window(&mut cx, |state, _, window| {
            let shape = |body| {
                GlyphRun::new(window.text_system().layout_line(
                    "a.txt",
                    px(body),
                    &[TextRun {
                        len: 5,
                        font: state.typeface.font.clone(),
                        color: Hsla::default(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                ))
            };
            let prepared = Prepared {
                name: shape(14.),
                ..Prepared::default()
            };
            assert!(prepared.bold.set(shape(20.)).is_ok());
            let mut shown = Shown {
                index: 0,
                view: 0,
                prepared: Some(Rc::new(prepared)),
                clip: Some(ClipMark::Copy),
                state: RowState {
                    cursor: true,
                    ..RowState::default()
                },
                bold_match: true,
                picture: None,
            };
            let name = Bounds::new(point(px(46.), px(0.)), size(px(200.), px(31.)));
            let cursor = shown.clip_bounds(name, &state.drawn.metrics).unwrap();
            let prepared = shown.prepared.as_ref().unwrap();
            saw_bold_advance |=
                prepared.bold.get().unwrap().line().width != prepared.name.line().width;
            let metrics = &state.drawn.metrics;
            let room = (name.size.width - px(metrics.gap + metrics.clip)).max(px(0.));
            assert_eq!(
                cursor.left(),
                name.left() + prepared.bold.get().unwrap().line().width.min(room) + px(metrics.gap)
            );
            shown.state.cursor = false;
            shown.bold_match = false;
            let plain = shown.clip_bounds(name, &state.drawn.metrics).unwrap();
            assert_eq!(
                plain.left(),
                name.left() + prepared.name.line().width.min(room) + px(metrics.gap)
            );
            assert!(
                cursor.left() > plain.left(),
                "the longer drawn bold name moves its mark while both stay inside the column"
            );
        });
    }
    assert!(
        saw_bold_advance,
        "the cursor fixture must distinguish the drawn bold run from the plain run"
    );
}
