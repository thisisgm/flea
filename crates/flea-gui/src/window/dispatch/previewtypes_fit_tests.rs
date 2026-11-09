//! A tall preview keeps its name and whole facts inside the column at every window height (P11).
use super::previewtypes_tests::{answer, ask, field, file, local, meta, open_at, pump, thumbed};
use crate::window::u07_tests::read;
use gpui::{px, size, Bounds, Pixels, TestAppContext, VisualTestContext};

#[path = "lang_polish_tests.rs"]
mod lang_polish_tests;

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
}

/// The bottom edge of an IPC rect "x y w h".
fn bottom(rect: &str) -> f32 {
    let n: Vec<f32> = rect
        .split(' ')
        .map(|n| n.parse().unwrap_or_else(|_| panic!("a rect, not {rect:?}")))
        .collect();
    n[1] + n[3]
}

fn assert_fits(shell: &gpui::Entity<crate::state::Shell>, cx: &mut VisualTestContext, at: &str) {
    let column = bounds(cx, "column-preview");
    let name = bounds(cx, "column-preview-name");
    let facts = bounds(cx, "column-preview-facts");
    let frame = bottom(&read(shell, cx, "columnFrameRect", &[]));
    let (column_bottom, name_top) = (f32::from(column.bottom()), f32::from(name.top()));
    let (name_bottom, facts_top) = (f32::from(name.bottom()), f32::from(facts.top()));
    let facts_bottom = f32::from(facts.bottom());
    assert!(
        frame <= name_top + 0.5,
        "{at}: the frame ends above the name: {frame} > {name_top}"
    );
    assert!(
        name_bottom <= facts_top + 0.5,
        "{at}: the facts sit under the name: {name_bottom} > {facts_top}"
    );
    let pad = shell.read_with(cx, |shell, cx| {
        let face = shell
            .selection_preview(0)
            .expect("the column is mounted")
            .read(cx)
            .face
            .clone();
        crate::list::Metrics::for_typeface(&face).pad
    });
    assert!(
        facts_bottom <= column_bottom - pad + 0.5,
        "{at}: the whole facts line keeps the column's bottom margin {pad}: {facts_bottom} vs {column_bottom}"
    );
}

#[gpui::test]
fn a_code_preview_keeps_its_name_and_facts_inside_the_column_at_every_height(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open_at(cx, "/files", Some(14.));
    let rows = vec![file(
        "code-rs.rs",
        "text-x-rust",
        "Rust source code",
        0o100644,
    )];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let lines = local(&shell, cx, "/files");
    meta(&shell, cx, &lines, r#","lines":3"#);
    let code = ask(&lines, "code").to_owned();
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"styled","first":1,"total":3,"lines":[[5,"fn",0," a()"],[0,"}}"],[]]}}"#,
            field(&code, "token")
        ),
    );
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "code");
    // Compact facts first, then the full table that i turns on.
    for facts in ["compact", "full"] {
        for (w, h) in [(1500., 1013.), (2400., 1300.), (1100., 480.), (900., 360.)] {
            cx.simulate_resize(size(px(w), px(h)));
            pump(cx);
            assert_fits(&shell, cx, &format!("{facts} {w}x{h}"));
            if facts == "compact" && w > 900. {
                let line = bounds(cx, "column-preview-facts").size.height;
                assert_eq!(
                    f32::from(line),
                    18.,
                    "the board's compact facts line is 18 px"
                );
            }
        }
        cx.simulate_keystrokes("i->i");
        pump(cx);
    }
}

/// cols-preview-video.html at text 14: 18 px rows 21 px apart, values at the 71 px label column, no play mark on the frame.
#[gpui::test]
fn video_facts_take_the_board_pitch_and_the_frame_draws_no_play_mark(cx: &mut TestAppContext) {
    use crate::g2::media_preview::view::Picture;
    let (shell, cx) = open_at(cx, "/files", Some(14.));
    let rows = vec![file("clip.webm", "video-webm", "WebM video", 0o100644)];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let lines = local(&shell, cx, "/files");
    meta(&shell, cx, &lines, r#","w":640,"h":360,"ms":4000"#);
    thumbed(&shell, cx, &lines, 0, "");
    cx.simulate_resize(size(px(1500.), px(1013.)));
    pump(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "video");
    let count = read(&shell, cx, "previewFacts", &[]).split('|').count();
    assert!(count >= 5, "Kind, Size, Modified, Duration and Pixels");
    const ROWS: [&str; 5] = [
        "column-preview-fact-0",
        "column-preview-fact-1",
        "column-preview-fact-2",
        "column-preview-fact-3",
        "column-preview-fact-4",
    ];
    let tops: Vec<f32> = ROWS
        .iter()
        .map(|row| f32::from(bounds(cx, row).top()))
        .collect();
    for (row, pair) in ROWS.iter().zip(tops.windows(2)) {
        assert_eq!(
            f32::from(bounds(cx, row).size.height),
            18.,
            "{row} is 18 px tall"
        );
        assert_eq!(
            pair[1] - pair[0],
            21.,
            "fact rows are 21 px apart: {tops:?}"
        );
    }
    let label = f32::from(bounds(cx, "column-preview-value-0").left())
        - f32::from(bounds(cx, ROWS[0]).left());
    assert_eq!(label, 71., "values start at the board's 71 px label column");
    let name = bounds(cx, "column-preview-name");
    assert_eq!(
        f32::from(name.size.height),
        20.,
        "the name is the board's 20 px line"
    );
    let poster = Picture::Video {
        frame: None,
        poster: true,
    };
    assert_eq!(
        crate::window::selection_preview::render::column_picture(poster),
        Picture::Video {
            frame: None,
            poster: false
        },
        "the column frame carries no play mark"
    );
}

/// Tamil's Modified is wider than the board's label column: the column widens to hold the widest label whole, and every value starts at one x.
#[gpui::test]
fn a_label_wider_than_the_board_column_widens_it_to_hold_that_label(cx: &mut TestAppContext) {
    let (shell, cx) = open_at(cx, "/files", Some(14.));
    let base =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/window/dispatch/fixtures/tamil");
    let tamil = flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: base.clone(),
        language_file: base.join("language"),
        catalogs: vec![base],
    });
    shell.update(cx, |shell, cx| {
        let catalog = std::sync::Arc::new(tamil);
        shell.apply_locale(
            crate::window::locale::Snapshot {
                revision: 2,
                catalog,
            },
            cx,
        )
    });
    let rows = vec![file("clip.webm", "video-webm", "WebM video", 0o100644)];
    super::test_shell::land(&shell, cx, "/files", rows, 1);
    let lines = local(&shell, cx, "/files");
    meta(&shell, cx, &lines, r#","w":640,"h":360,"ms":4000"#);
    thumbed(&shell, cx, &lines, 0, "");
    cx.simulate_resize(size(px(1500.), px(1013.)));
    pump(cx);
    let facts = read(&shell, cx, "previewFacts", &[]);
    let labels: Vec<String> = facts
        .split('|')
        .filter_map(|fact| fact.split_once('=').map(|(label, _)| label.to_owned()))
        .collect();
    let (face, board) = shell.read_with(cx, |shell, cx| {
        let preview = shell
            .selection_preview(0)
            .expect("the column is mounted")
            .read(cx);
        (
            preview.face.clone(),
            preview
                .fact_grid(crate::window::selection_preview::Kind::Video)
                .1,
        )
    });
    let widest = cx.update(|window, _| {
        let measure = |label: &String| {
            let font = face.font.clone();
            crate::g2::status_feedback::caption::measure(
                label.clone().into(),
                font,
                face.caption,
                px(0.),
                window,
            )
        };
        labels.iter().map(measure).fold(0., f32::max)
    });
    assert!(
        widest > board,
        "{facts}: a Tamil label passes the board's {board} px column, so this test proves something"
    );
    const ROWS: [(&str, &str); 5] = [
        ("column-preview-fact-0", "column-preview-value-0"),
        ("column-preview-fact-1", "column-preview-value-1"),
        ("column-preview-fact-2", "column-preview-value-2"),
        ("column-preview-fact-3", "column-preview-value-3"),
        ("column-preview-fact-4", "column-preview-value-4"),
    ];
    let start = f32::from(bounds(cx, ROWS[0].1).left() - bounds(cx, ROWS[0].0).left());
    assert!(
        start >= widest,
        "the label column {start} holds the widest label {widest}"
    );
    for (row, value) in ROWS {
        let (row, value) = (bounds(cx, row), bounds(cx, value));
        assert_eq!(
            f32::from(value.left() - row.left()),
            start,
            "every value starts at one x"
        );
    }
}

#[test]
fn column_document_and_picture_insets_use_the_named_frame_border() {
    // The document inset is in selection_preview_render.rs, the still fit in selection_preview_fitted.rs and the moving fit in selection_preview_picture.rs; each is pinned in its own file.
    let sources = [
        ("document", include_str!("../selection_preview_render.rs")),
        ("still", include_str!("../selection_preview_fitted.rs")),
        ("moving", include_str!("../selection_preview_picture.rs")),
    ];
    for (inset, source) in sources {
        for dimension in ["width", "height"] {
            assert!(
                !source.contains(&format!("({dimension} - 2.)")),
                "the {inset} {dimension} must inset by the named frame border on both sides"
            );
            assert_eq!(
                source
                    .matches(&format!("({dimension} - 2. * FRAME_BORDER)"))
                    .count(),
                1,
                "the {inset} must inset its {dimension} by the frame border"
            );
        }
    }
}
