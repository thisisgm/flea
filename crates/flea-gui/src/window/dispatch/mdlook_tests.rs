use crate::g2::{pv1::doc::View, pv2::markdown::MarkdownView};
use flea::ui_logic_pv2::markdown::{
    markdown_flow::{Atom, Face, Ink, Laid, Measure},
    markdown_layout::Item,
    FigureKind,
};

const COLORS: &str = "background='#111c18'\ndark_background='#0c1512'\nforeground='#c1c497'\nbright_foreground='#f7e8b2'\nmuted='#5f6f66'\naccent='#58a080'";

fn markdown<'a>(
    cx: &'a mut TestAppContext,
    root: &Path,
    source: &str,
) -> (
    Entity<Shell>,
    &'a mut VisualTestContext,
    Entity<MarkdownView>,
) {
    cx.executor().allow_parking();
    std::fs::write(root.join("look.md"), source).unwrap();
    let (shell, cx, _) = opened_at(cx, root);
    cx.update(|_, cx| cx.set_global(crate::palette::Theme::from_text(COLORS)));
    land(
        &shell,
        cx,
        &root.to_string_lossy(),
        vec![file("look.md", "text-markdown")],
        1,
    );
    cx.simulate_keystrokes("space");
    draw(cx);
    let Some(Body::Doc(doc)) = body(&shell, cx) else {
        panic!("Markdown card mounted")
    };
    let View::Markdown(view) = doc.view else {
        panic!("Markdown view mounted")
    };
    wait(cx, "the mounted document is laid and shown", |cx| {
        view.read_with(cx, |view, _| {
            (view.doc.shown && view.doc.complete && view.doc.laid.iter().all(Option::is_some))
                .then_some(())
        })
    });
    (shell, cx, view)
}

fn texts(items: &[Item], out: &mut Vec<(f32, f32, Laid)>) {
    for item in items {
        match item {
            Item::Text { x, y, laid } => out.push((*x, *y, laid.clone())),
            Item::Table { items, .. } => texts(items, out),
            _ => {}
        }
    }
}

fn laid(view: &Entity<MarkdownView>, cx: &VisualTestContext) -> Vec<(f32, f32, Laid)> {
    view.read_with(cx, |view, _| {
        let mut out = Vec::new();
        for block in view.doc.laid.iter().flatten() {
            texts(&block.geometry.items, &mut out);
        }
        out
    })
}

fn chip_padding(view: &Entity<MarkdownView>, cx: &mut VisualTestContext, expected: usize) {
    let lines = laid(view, cx);
    let mut chips = 0;
    for (_, _, laid) in lines {
        for line in laid.lines {
            for atom in &line.atoms {
                let Atom::Ground { x, width, .. } = atom else {
                    continue;
                };
                let runs: Vec<_> = line
                    .atoms
                    .iter()
                    .filter_map(|atom| match atom {
                        Atom::Text {
                            x: at, text, look, ..
                        } if look.chip && !text.is_empty() && *at >= *x && *at < *x + *width => {
                            Some((*at, text, look))
                        }
                        _ => None,
                    })
                    .collect();
                if runs.is_empty() {
                    continue;
                }
                let left = runs[0].0 - x;
                let (at, text, look) = runs.last().unwrap();
                let advance = view.update(cx, |view, _| {
                    view.source.measure().width(text, look.face, look.px)
                });
                let right = x + width - at - advance;
                assert!(
                    (left - 4.).abs() < 0.1 && (right - 4.).abs() < 0.1,
                    "chip has 4 px on each side, got left={left} right={right}"
                );
                chips += 1;
            }
        }
    }
    assert_eq!(chips, expected);
}

#[gpui::test]
fn mdlook_ey155_headings_use_the_bright_theme_ink(cx: &mut TestAppContext) {
    let root = root();
    let (shell, cx, view) = markdown(
        cx,
        &root.0,
        "# Heading\n\n**Body**\n\n| Header |\n|---|\n| Cell |\n",
    );
    view.read_with(cx, |view, _| {
        assert_eq!(
            crate::g2::pv2::markdown::style::hex(view.palette.ink(&Ink::Bright)),
            "#f7e8b2"
        );
        assert_eq!(
            crate::g2::pv2::markdown::style::hex(view.palette.foreground),
            "#c1c497"
        );
    });
    assert!(laid(&view, cx).iter().any(|(_, _, laid)| laid.lines.iter().any(|line| line.atoms.iter().any(
        |atom| matches!(atom, Atom::Text { text, look, .. } if text == "Heading" && look.ink == Ink::Bright)))));
    assert!(laid(&view, cx).iter().any(|(_, _, laid)| laid.lines.iter().any(|line| line.atoms.iter().any(
        |atom| matches!(atom, Atom::Text { text, look, .. } if text == "Body" && look.ink == Ink::Foreground)))));
    close(&shell, cx);
}

#[gpui::test]
fn mdlook_ey156_keycaps_leave_the_plus_between_two_padded_caps(cx: &mut TestAppContext) {
    let root = root();
    let (shell, cx, view) = markdown(cx, &root.0, "Press <kbd>Ctrl</kbd>+<kbd>C</kbd> to copy.\n");
    chip_padding(&view, cx, 2);
    let lines = laid(&view, cx);
    let atoms = &lines[0].2.lines[0].atoms;
    let plus = atoms
        .iter()
        .find_map(|atom| match atom {
            Atom::Text { x, text, look, .. } if text == "+" && !look.chip => Some(*x),
            _ => None,
        })
        .expect("plus outside caps");
    assert!(atoms.iter().all(|atom| match atom {
        Atom::Ground { x, width, .. } => plus < *x || plus >= x + width,
        _ => true,
    }));
    close(&shell, cx);
}

#[gpui::test]
fn mdlook_ey157_inline_code_has_even_padding_in_prose_and_table_cells(cx: &mut TestAppContext) {
    let root = root();
    let (shell, cx, view) = markdown(
        cx,
        &root.0,
        "Some `inline code` and text.\n\n| Code |\n|---|\n| `a \\| b` |\n",
    );
    chip_padding(&view, cx, 2);
    close(&shell, cx);
}

#[gpui::test]
fn mdlook_ey158_figures_use_the_muted_ink_and_unfilled_ground(cx: &mut TestAppContext) {
    let root = root();
    let (shell, cx, view) = markdown(cx, &root.0, "Figures\n");
    let (theme, ground, accent) = view.update(cx, |view, _| {
        (
            view.figure_theme(FigureKind::Mermaid),
            crate::g2::pv2::markdown::style::hex(view.palette.background),
            crate::g2::pv2::markdown::style::hex(view.palette.accent_frame),
        )
    });
    let muted =
        crate::g2::pv2::markdown::style::hex(crate::palette::Theme::from_text(COLORS).muted);
    eprintln!("figure muted request={} ground={ground}", theme.muted);
    assert_eq!(theme.muted, muted, "diagram border follows the muted paint");
    assert_ne!(ground, "none");
    assert_eq!(
        theme.surface, "none",
        "figure box keeps its surrounding ground"
    );
    assert_eq!(
        theme.accent, accent,
        "arrowheads retain the HIG contrast floor"
    );
    close(&shell, cx);
}

#[gpui::test]
fn mdlook_ey160_small_images_keep_their_own_pixels_and_large_fit_is_preserved(
    cx: &mut TestAppContext,
) {
    let root = root();
    let (shell, cx, mut requests) = opened_at(cx, &root.0);
    png(&root.0.join("logo.png"), 96, 48);
    png(&root.0.join("large.png"), 4000, 3000);
    land(
        &shell,
        cx,
        &root.0.to_string_lossy(),
        vec![
            file("logo.png", "image-png"),
            file("large.png", "image-png"),
        ],
        1,
    );
    for (index, sw, sh) in [(0, 96, 48), (1, 4000, 3000)] {
        set_cursor(&shell, cx, index);
        cx.simulate_keystrokes("space");
        draw(cx);
        let meta = sent(&mut requests, "meta");
        assert_eq!(meta.len(), 1);
        answer(&shell, cx, meta_reply(index, sw, sh, token(&meta[0])));
        wait(cx, "the image picture", |cx| cx.debug_bounds("pv1-picture"));
        let card = cx.debug_bounds("pv1-card").unwrap();
        let picture = cx.debug_bounds("pv1-picture").unwrap();
        let (w, h) = (f32::from(card.size.width), f32::from(card.size.height));
        let scale = cx.update(|window, _| window.scale_factor());
        // A raster never draws past one source pixel per device pixel; a large one fits the card.
        let factor = (w / sw as f32).min(h / sh as f32).min(1. / scale);
        assert!((f32::from(picture.size.width) - sw as f32 * factor).abs() < 1.);
        assert!((f32::from(picture.size.height) - sh as f32 * factor).abs() < 1.);
        cx.simulate_keystrokes("escape");
        draw(cx);
    }
    close(&shell, cx);
}

#[gpui::test]
fn mdlook_ey163_wide_table_scrolls_with_whole_headers(cx: &mut TestAppContext) {
    let root = root();
    let header = (0..4)
        .map(|i| format!("Column{i}_with_a_long_header"))
        .collect::<Vec<_>>()
        .join(" | ");
    let source = format!("| {header} |\n|---|---|---|---|\n| value_0_0_long_cell | value_0_1_long_cell | value_0_2_long_cell | value_0_3_long_cell |\n");
    let (shell, cx, view) = markdown(cx, &root.0, &source);
    let texts = laid(&view, cx);
    let heads: Vec<_> = texts
        .iter()
        .filter(|(_, _, laid)| {
            laid.lines.iter().any(|line| {
                line.atoms
                    .iter()
                    .any(|atom| matches!(atom, Atom::Text { look, .. } if look.face == Face::Bold))
            })
        })
        .collect();
    assert_eq!(heads.len(), 4);
    assert!(
        heads.iter().all(|(_, _, laid)| laid.lines.len() == 1),
        "each column holds its longest header token whole, as v0.3.12 does"
    );
    for (index, (_, _, laid)) in heads.iter().enumerate() {
        let shown: String = laid
            .lines
            .iter()
            .flat_map(|line| &line.atoms)
            .filter_map(|atom| {
                if let Atom::Text { text, .. } = atom {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(shown, format!("Column{index}_with_a_long_header"));
    }
    view.read_with(cx, |view, _| {
        let overflowing = view
            .doc
            .laid
            .iter()
            .flatten()
            .flat_map(|block| &block.geometry.items)
            .filter(|item| matches!(item, Item::Table { content, width, .. } if content > width))
            .count();
        assert_eq!(
            overflowing, 1,
            "a table wider than the card scrolls sideways instead of splitting its tokens"
        );
    });
    close(&shell, cx);
}

#[gpui::test]
fn mdlook_ey163_table_headers_wrap_at_words_before_splitting_tokens(cx: &mut TestAppContext) {
    let root = root();
    let header = "alpha beta gamma delta epsilon zeta";
    let source = format!("| {header} | {header} | {header} | {header} |\n|---|---|---|---|\n| one | two | three | four |\n");
    let (shell, cx, view) = markdown(cx, &root.0, &source);
    let words: Vec<_> = header.split_whitespace().collect();
    let mut wrapped = 0;
    for (_, _, laid) in laid(&view, cx) {
        if laid.lines.len() < 2 {
            continue;
        }
        wrapped += 1;
        for line in laid.lines {
            for atom in line.atoms {
                if let Atom::Text { text, .. } = atom {
                    assert!(
                        text.split_whitespace().all(|word| words.contains(&word)),
                        "whole words wrap before token splitting: {text}"
                    );
                }
            }
        }
    }
    assert_eq!(wrapped, 4);
    close(&shell, cx);
}

#[gpui::test]
fn mdlook_ey332_source_sets_lines_on_the_code_views_row(cx: &mut TestAppContext) {
    let root = root();
    let (shell, cx, view) = markdown(cx, &root.0, "# Release notes\n\nA line.\n");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_view(crate::g2::pv2::markdown::View::Source, cx)
        })
    });
    draw(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.source.frame.line),
        crate::g2::pv1::look::TEXT_LINE,
        "ql-markdown-source and ql-code both set lines 20 apart"
    );
    close(&shell, cx);
}
