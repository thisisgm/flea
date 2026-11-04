//! A name whose glyphs draw wider than one cell: crumbs and suggestion pieces sit on their shaped widths, and a press opens what is drawn under it.
use super::{
    inputs,
    jump::{answer, Folders},
    view::{bar, Bar},
    HOME,
};
use flea_gui::{
    ipc::{path::PathAdapter, Adapter},
    path::{paint::Size, suggestions::Unranked, view::Out, Answer},
};
use gpui::TestAppContext;

// Three folder glyphs: the test text system shapes each two cells wide, as a CJK or emoji fallback face does.
const WIDE: &str = "\u{1F4C1}\u{1F4C1}\u{1F4C1}";

fn widths(bar: &mut Bar<'_>) -> Vec<f32> {
    bar.read(|view| {
        view.paint.borrow().runs[0]
            .iter()
            .map(|run| f32::from(run.line().width))
            .collect()
    })
}

#[gpui::test]
fn a_wide_crumb_pushes_its_neighbour_along_and_a_press_on_its_glyphs_opens_it(
    cx: &mut TestAppContext,
) {
    let mut bar = bar(
        cx,
        Box::new(Unranked),
        inputs(&format!("{HOME}/{WIDE}/next/leaf")),
    );
    let cell = bar.advance(Size::Caption);
    let texts = bar.read(|view| {
        view.strips[0]
            .crumbs()
            .iter()
            .map(|crumb| crumb.text.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(texts, ["~/", &format!("{WIDE}/"), "next/", "leaf"]);
    let drawn = widths(&mut bar);
    assert!(
        drawn[1] > 4. * cell + 1.,
        "the wide crumb draws wider than its four characters: {drawn:?} at {cell}"
    );
    let extents: Vec<_> = bar.read(|view| {
        (0..4)
            .map(|index| view.strips[0].extent(index).unwrap())
            .collect()
    });
    let mut left = 0.;
    for (index, (start, width)) in extents.iter().enumerate() {
        let near = (start - left).abs() < 0.01 && (width - drawn[index]).abs() < 0.01;
        assert!(
            near,
            "crumb {index} sits right after the one before it: {start} {width} against {left} {}",
            drawn[index]
        );
        left += drawn[index];
    }
    // The last drawn folder glyph of crumb 1, which the cell grid gave to crumb 2.
    let (start, width) = extents[1];
    bar.press(start + width * 0.8, 13., 1);
    assert_eq!(
        bar.take(),
        [Out::Open {
            side: 0,
            path: format!("{HOME}/{WIDE}")
        }]
    );
    bar.press(extents[2].0 + 1., 13., 1);
    assert_eq!(
        bar.take(),
        [Out::Open {
            side: 0,
            path: format!("{HOME}/{WIDE}/next")
        }]
    );
    // The reader a driven test presses by answers the drawn centre too.
    let centre = bar
        .read(|view| PathAdapter { view }.call("crumbCentre", &["2".into()]))
        .unwrap();
    let want = format!(
        "{} 13",
        (extents[2].0 + extents[2].1 / 2. + 0.5).floor() as i64
    );
    assert_eq!(centre.stdout.trim_end_matches('\n'), want);
}

#[gpui::test]
fn a_redraw_of_an_unchanged_strip_reshapes_nothing_and_keeps_its_layout(cx: &mut TestAppContext) {
    let mut bar = bar(
        cx,
        Box::new(Unranked),
        inputs(&format!("{HOME}/{WIDE}/next/leaf")),
    );
    let state = |bar: &mut Bar<'_>| {
        bar.read(|view| {
            let paint = view.paint.borrow();
            (
                paint.runs[0].len(),
                paint.shapes.misses,
                view.strips[0].extent(3),
                view.strips[0].built,
            )
        })
    };
    let kept = state(&mut bar);
    assert_eq!(kept.0, 4);
    for _ in 0..5 {
        bar.view.update(bar.cx, |_, cx| cx.notify());
        bar.cx.run_until_parked();
    }
    assert_eq!(
        state(&mut bar),
        kept,
        "a redraw reshaped, refit or moved the strip"
    );
}

#[gpui::test]
fn a_wide_suggestion_lays_each_piece_after_the_one_before_it(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(Folders::default()), inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    let wide = format!("/srv/{WIDE}/omarchy");
    let zoxide = [wide.as_str(), "/home/gm/Music"];
    bar.update(|view, window, cx| view.receive(Answer::Jumped(answer(1, &zoxide)), window, cx));
    bar.update(|view, _, cx| view.history_read(Vec::new(), cx));
    bar.update(|view, window, cx| view.receive(Answer::Jumped(answer(2, &zoxide)), window, cx));
    bar.cx.simulate_input("om");
    bar.cx.run_until_parked();
    let row = bar
        .read(|view| view.jump.entries.iter().position(|row| row.path == wide))
        .expect("the wide row ranks");
    let view = bar.view.clone();
    let narrow = bar.cx.update(|window, cx| {
        f32::from(
            view.read(cx)
                .paint
                .borrow_mut()
                .shape("/srv/abc/", Size::Body, false, window)
                .line()
                .width,
        )
    });
    let pieces: Vec<(f32, f32, bool)> = bar.read(|view| {
        view.paint
            .borrow()
            .pieces
            .iter()
            .filter(|piece| piece.row == row)
            .map(|piece| (piece.x, piece.width(), piece.leaf))
            .collect()
    });
    assert!(pieces.len() >= 2, "{pieces:?}");
    let (ancestors, leaf) = (
        pieces[0],
        pieces.iter().find(|piece| piece.2).copied().unwrap(),
    );
    // "/srv/", the three wide glyphs and "/" are nine characters, drawn wider than nine narrow ones.
    assert!(ancestors.1 > narrow + 1., "{pieces:?} against {narrow}");
    let mut x = 0.;
    for (start, width, _) in &pieces {
        assert_eq!(
            *start, x,
            "a piece starts where the one before it ends: {pieces:?}"
        );
        x += width;
    }
    assert_eq!(
        leaf.0,
        ancestors.0 + ancestors.1,
        "the leaf starts after the wide ancestors, never over them"
    );
}
