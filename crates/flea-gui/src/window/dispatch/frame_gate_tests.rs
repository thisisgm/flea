//! Every reader mounts through the production Shell and supplies an actual frame receipt.
use super::{
    preview_tests::{close, draw},
    quick_look_body_tests::{mounted, open_kind},
};
use crate::presentation::frame_gate;
use gpui::{px, size, TestAppContext};
use std::time::Duration;

const FRAMELESS_READERS: &[(&str, &str)] =
    &[("book.pdf", "ql-pdf"), ("report.docx", "ql-office-doc")];
// Longer than every PreviewSwap step (150 ms hold plus 100 ms minimum show), so each frame is read at rest.
const SETTLE: Duration = Duration::from_millis(400);

fn check(cx: &mut TestAppContext, name: &str, file: &str) {
    let (shell, cx, _requests, _script) = mounted(cx);
    cx.simulate_resize(size(px(1100.), px(700.)));
    let theme = frame_gate::theme();
    cx.update(|window, cx| {
        cx.set_global(theme);
        shell.update(cx, |shell, cx| shell.apply_corner(8., window, cx));
    });
    open_kind(&shell, cx, file);
    for radius in [8., 0., 13.] {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.apply_corner(radius, window, cx))
        });
        draw(cx);
        cx.executor().advance_clock(SETTLE);
        draw(cx);
        let bounds = cx
            .debug_bounds("pv1-card")
            .expect("real Quick Look card mounted");
        if let Some((_, board)) = FRAMELESS_READERS.iter().find(|(reader, _)| *reader == file) {
            frame_gate::assert_unframed(name, cx, bounds, board);
        } else {
            frame_gate::assert_frame(name, cx, bounds, radius, theme);
        }
    }
    close(&shell, cx);
}

macro_rules! reader {
    ($test:ident, $file:literal) => {
        #[gpui::test]
        fn $test(cx: &mut TestAppContext) {
            check(cx, stringify!($test), $file);
        }
    };
}
reader!(vf_frames_ql_audio, "song.ogg");
reader!(vf_frames_ql_video, "clip.mp4");
reader!(vf_frames_ql_markdown, "readme.md");
reader!(vf_frames_ql_code, "main.rs");
reader!(vf_frames_ql_html, "page.html");
reader!(vf_frames_ql_csv, "table.csv");
reader!(vf_frames_ql_office, "report.docx");
reader!(vf_frames_ql_pdf, "book.pdf");
reader!(vf_frames_ql_font, "Inter-Bold.ttf");
reader!(vf_frames_ql_archive, "pack.zip");
reader!(vf_frames_ql_model, "benchy.stl");

#[gpui::test]
fn vf_frames_ql_first_open_takes_the_live_corner(cx: &mut TestAppContext) {
    use super::{
        preview_tests::{file, opened},
        test_shell::{land, set_cursor},
    };
    let (shell, cx, _requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain")],
        1,
    );
    set_cursor(&shell, cx, 0);
    let theme = frame_gate::theme();
    cx.update(|window, cx| {
        cx.set_global(theme);
        shell.update(cx, |shell, cx| {
            shell.visual.scene_corner = Some(13.);
            shell.apply_corner(8., window, cx);
        });
    });
    // The compositor says 8 but this window's scene says 13, and only the open path hands the new card 13.
    cx.simulate_keystrokes("space");
    draw(cx);
    cx.executor().advance_clock(SETTLE);
    draw(cx);
    let bounds = cx
        .debug_bounds("pv1-card")
        .expect("real Quick Look card mounted");
    frame_gate::assert_frame("ql-first-open", cx, bounds, 13., theme);
    close(&shell, cx);
}

fn menu(cx: &mut TestAppContext, header: bool) {
    use super::{
        menus_tests::{answer, entry, settle, tags_answer},
        test_shell::{land, open_root},
    };
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![entry("notes.txt", 0o100644)], 1);
    let theme = frame_gate::theme();
    cx.update(|window, cx| {
        cx.set_global(theme);
        cx.set_global(frame_gate::Marks::default());
        shell.update(cx, |shell, cx| {
            shell.apply_corner(8., window, cx);
            if header {
                shell.header_menu_open(
                    0,
                    &gpui::MouseDownEvent {
                        button: gpui::MouseButton::Right,
                        position: gpui::point(px(500.), px(40.)),
                        ..Default::default()
                    },
                    window,
                    cx,
                );
            } else {
                shell.menu_keyboard_open(window, cx);
            }
        });
    });
    if !header {
        answer(&shell, cx, &[0o100000]);
        tags_answer(&shell, cx, &[0o100000]);
    }
    settle(cx);
    let frames = shell.read_with(cx, |shell, _| {
        let menu = shell.menu.menu.borrow();
        let card = menu.card.as_ref().expect("real menu mounted");
        assert!(card
            .built
            .lines
            .iter()
            .any(|line| !matches!(line.lead, crate::g2::mn1::lines::Lead::None)));
        gpui::Bounds::new(
            gpui::point(px(card.frame.x), px(card.frame.y)),
            size(px(card.frame.w), px(card.frame.h)),
        )
    });
    frame_gate::assert_frame(
        if header {
            "header-menu-pointer"
        } else {
            "tags-parent-menu"
        },
        cx,
        frames,
        8.,
        theme,
    );
    cx.read(|cx| {
        let receipts = cx.global::<frame_gate::Marks>().0.borrow();
        assert!(!receipts.is_empty(), "menu glyph paint receipts required");
        for (id, ink) in receipts.iter().filter(|(id, _)| header || id == "tags") {
            assert_eq!(*ink, theme.muted_frame, "{id}: painted leading glyph role");
        }
        if !header {
            assert!(
                receipts.iter().any(|(id, _)| id == "tags"),
                "Tags parent glyph painted"
            );
        }
    });
    close(&shell, cx);
}

#[gpui::test]
fn vf_frames_header_menu_glyphs(cx: &mut TestAppContext) {
    menu(cx, true);
}
#[gpui::test]
fn vf_frames_tags_parent_menu_glyph(cx: &mut TestAppContext) {
    menu(cx, false);
}

#[gpui::test]
fn vf_frames_compare_binary(cx: &mut gpui::TestAppContext) {
    use super::{
        compare_tests::{compare, pair},
        preview_tests::opened,
    };
    let (shell, cx, mut requests) = opened(cx);
    pair(&shell, cx);
    let theme = frame_gate::theme();
    cx.update(|window, cx| {
        cx.set_global(theme);
        shell.update(cx, |shell, cx| shell.apply_corner(8., window, cx));
    });
    compare(&shell, cx, &mut requests, "binary", "");
    cx.executor().advance_clock(SETTLE);
    draw(cx);
    let bounds = cx.debug_bounds("pv1-card").unwrap();
    crate::presentation::frame_gate::assert_frame("compare-binary", cx, bounds, 8., theme);
    close(&shell, cx);
}

#[gpui::test]
fn vf_frames_main_views(cx: &mut TestAppContext) {
    use super::{
        preview_tests::file,
        test_shell::{land, open_root_with},
    };
    for view in ["list", "columns", "grid", "dual"] {
        let (shell, cx) = open_root_with(cx, &format!(r#""view":"{view}""#), None);
        land(
            &shell,
            cx,
            "/files",
            vec![file("notes.txt", "text-plain")],
            1,
        );
        for radius in [0., 8.] {
            cx.update(|window, cx| {
                shell.update(cx, |shell, cx| shell.apply_corner(radius, window, cx))
            });
            draw(cx);
            // HIG list, cols-facts-full, grid and dual: rows and full-width strips stay square.
            let (_, quads) = frame_gate::capture(cx);
            assert!(!quads.is_empty(), "{view}: real root painted");
            assert!(
                quads
                    .iter()
                    .filter(|q| q.border_widths.top.0 == 0.)
                    .all(|q| q.corner_radii.top_left.0 == 0.),
                "{view}: unframed rows and bars stay square"
            );
            println!("VF_FRAMES surface=main-{view} radius={radius} unframed=HIG PASS");
        }
        close(&shell, cx);
    }
}

#[gpui::test]
fn vf_frames_preview_column(cx: &mut TestAppContext) {
    use super::{
        previewtypes_tests::{answer, ask, field, file, local, meta, open, pump},
        test_shell::land,
    };
    cx.executor().allow_parking();
    let (shell, cx) = open(cx, "/files");
    let theme = frame_gate::theme();
    cx.update(|window, cx| {
        cx.set_global(theme);
        shell.update(cx, |shell, cx| shell.apply_corner(8., window, cx));
    });
    land(
        &shell,
        cx,
        "/files",
        vec![file("notes.txt", "text-plain", "Text", 0o100644)],
        1,
    );
    let asks = local(&shell, cx, "/files");
    let code = ask(&asks, "code").to_owned();
    meta(&shell, cx, &asks, r#","lines":1"#);
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"code","token":{},"slot":"column","state":"plain","first":1,"total":1,"lines":[[0,"notes"]]}}"#,
            field(&code, "token")
        ),
    );
    pump(cx);
    for radius in [8., 0., 13.] {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.apply_corner(radius, window, cx))
        });
        draw(cx);
        cx.executor().advance_clock(SETTLE);
        draw(cx);
        let bounds = cx
            .debug_bounds("column-preview-frame")
            .expect("real preview frame mounted");
        frame_gate::assert_frame("preview-column", cx, bounds, radius, theme);
    }
    close(&shell, cx);
}
