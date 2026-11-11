//! FIX-C2: the chooser's bands, Recent List names, the Save As row, a refused label and the narrow path strip, through the real view.
use super::{
    process::Line,
    tests::fixture::{fixture, Fixture},
};
use crate::{g2::status_feedback::caption::INKED, list::Palette, palette::Theme};
use gpui::{px, size, Bounds, Hsla, Pixels, TestAppContext};

const TEXT_FILES: &str = r#", "filters":[{"label":"Text files","globs":["*.txt"]}]"#;

fn same(a: Hsla, b: Hsla) -> bool {
    (a.h - b.h).abs() < 1e-3
        && (a.s - b.s).abs() < 1e-3
        && (a.l - b.l).abs() < 1e-3
        && (a.a - b.a).abs() < 1e-3
}

fn draw(f: &mut Fixture) {
    f.cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    f.cx.run_until_parked();
}

fn palette(f: &mut Fixture) -> (Palette, Theme) {
    f.cx.update(|window, cx| {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        (Palette::inherited(&window.text_style(), theme), theme)
    })
}

#[gpui::test]
fn picker_c2_title_answer_status_and_filter_row_draw_on_the_surface(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        &format!(r#"{{"mode":"save","folder":"/d","name":"sa-d3-new.txt"{TEXT_FILES}}}"#),
        None,
    );
    f.cx.simulate_resize(size(px(720.), px(700.)));
    f.cx.run_until_parked();
    draw(&mut f);
    let (_, theme) = palette(&mut f);
    assert!(
        !same(theme.surface, theme.background),
        "the fixture's theme tells the surface from the pane"
    );
    let bands = [
        "picker-title-band",
        "picker-filter-row",
        "picker-answers-band",
        "picker-status-band",
    ]
    .map(|band| {
        (
            band,
            f.cx.debug_bounds(band)
                .unwrap_or_else(|| panic!("{band} is mounted")),
        )
    });
    let (quads, scale) = f.cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        (window.painted_quads(), window.scale_factor())
    });
    let scaled = |bounds: Bounds<Pixels>| {
        [
            f32::from(bounds.origin.x) * scale,
            f32::from(bounds.origin.y) * scale,
            f32::from(bounds.size.width) * scale,
            f32::from(bounds.size.height) * scale,
        ]
    };
    for (band, bounds) in bands {
        let want = scaled(bounds);
        assert!(
            quads.iter().any(|quad| {
                let got = [
                    quad.bounds.origin.x.0,
                    quad.bounds.origin.y.0,
                    quad.bounds.size.width.0,
                    quad.bounds.size.height.0,
                ];
                got.iter().zip(want).all(|(a, b)| (a - b).abs() < 0.5)
                    && quad
                        .background
                        .as_solid()
                        .is_some_and(|color| same(color, theme.surface))
            }),
            "{band} paints its whole box on the surface, as the boards and v0.3.12 do"
        );
    }
}

// The start folder lands one file in List view, then Recent lists two history rows.
const ONE_FILE: &str = r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"notes.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-plain"}]}"#;

fn folder_list(cx: &mut TestAppContext) -> Fixture<'_> {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":1,"v":1,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(ONE_FILE.into()));
    f.cx.run_until_parked();
    f.update(|picker, cx| picker.user_set_view("list", cx));
    draw(&mut f);
    draw(&mut f);
    f
}

fn open_recent(f: &mut Fixture) {
    f.update(|picker, cx| picker.open_folder(flea::ui_logic_picker_portal::RECENT, cx));
    f.cx.run_until_parked();
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/","n":2,"v":2,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":2,"rows":[{"n":"home/u/Media/report.pdf","d":false,"s":3,"m":0,"p":33188,"i":"application-pdf"},{"n":"home/u/work/notes.txt","d":false,"s":5,"m":0,"p":33188,"i":"text-plain"}]}"#.into()));
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.total), 2);
    draw(f);
    draw(f);
}

fn name_title(f: &mut Fixture) -> String {
    let listing = f.read(|picker| picker.browse.listing.clone());
    f.cx.update(|_, cx| {
        let state = listing.read(cx).list_state();
        let state = state.borrow();
        state
            .header
            .name_title()
            .expect("the header was shaped")
            .to_owned()
    })
}

#[gpui::test]
fn picker_c2_recent_list_names_each_row_by_its_leaf(cx: &mut TestAppContext) {
    let mut f = folder_list(cx);
    open_recent(&mut f);
    let listing = f.read(|picker| picker.browse.listing.clone());
    f.cx.update(|_, cx| {
        let state = listing.read(cx).list_state();
        assert_eq!(
            state.borrow().drawn_names(),
            ["report.pdf", "notes.txt"],
            "picker-recent-list draws the file name, never the whole path"
        );
    });
}

#[gpui::test]
fn picker_c2_recent_list_header_draws_no_sort_arrow(cx: &mut TestAppContext) {
    let arrows = ['\u{25b4}', '\u{25be}'];
    let mut f = folder_list(cx);
    let folder = name_title(&mut f);
    assert!(
        folder.contains(arrows),
        "a folder sorted by Name carries the arrow: {folder:?}"
    );
    open_recent(&mut f);
    let recent = name_title(&mut f);
    assert!(
        !recent.contains(arrows),
        "a history has no sort order to show, so Name carries no arrow: {recent:?}"
    );
}

#[gpui::test]
fn picker_c2_save_as_label_is_muted_tracked_and_24_px_after_new_folder(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d","name":"sa-d3-new.txt"}"#,
        None,
    );
    f.cx.simulate_resize(size(px(1100.), px(700.)));
    f.cx.run_until_parked();
    let (palette, _) = palette(&mut f);
    INKED.with_borrow_mut(|inked| *inked = Some(Vec::new()));
    draw(&mut f);
    let inked = INKED.with_borrow_mut(|inked| inked.take().unwrap());
    let (_, ink, tracking, bounds) = inked
        .into_iter()
        .find(|(text, ..)| text.as_ref() == "SAVE AS")
        .expect("SAVE AS paints as a tracked caption");
    assert!(same(ink, palette.muted), "the label draws in muted ink");
    assert_eq!(tracking, px(1.2), "the board's 1.2 px caption tracking");
    let new_folder =
        f.cx.debug_bounds("picker-new-folder")
            .expect("New Folder… in the Save answer row");
    assert_eq!(
        bounds.left() - new_folder.right(),
        px(24.),
        "9 px gap, a 6 px spacer and 9 px gap stand between New Folder… and SAVE AS"
    );
    let field =
        f.cx.debug_bounds("pk1-filename")
            .expect("the Save As field");
    assert_eq!(
        field.left() - bounds.right(),
        px(9.),
        "the field stays 9 px after the label"
    );
}

// The ink the answer's label inherits in the real tree, read by its own drawn probe.
fn answer_ink(f: &mut Fixture) -> Hsla {
    f.update(|picker, _| picker.drawn.arm());
    draw(f);
    f.read(|picker| picker.drawn.answer_ink())
        .expect("the answer's probe painted")
}

#[gpui::test]
fn picker_c2_a_refused_answer_draws_its_label_in_muted_ink(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":0,"v":1,"w":true}"#.into(),
    ));
    f.cx.run_until_parked();
    let (palette, _) = palette(&mut f);
    assert!(
        !same(palette.muted, palette.foreground),
        "the fixture's palette tells muted from foreground"
    );
    assert!(
        !f.read(|picker| picker.can_accept()),
        "an Open chooser with nothing chosen refuses its answer"
    );
    assert!(
        same(answer_ink(&mut f), palette.muted),
        "DialogButton.qml inks an unavailable label muted, not foreground"
    );
    let mut f = folder_list(cx);
    assert!(
        f.read(|picker| picker.can_accept()),
        "the cursor on notes.txt makes Open available"
    );
    assert!(
        same(answer_ink(&mut f), palette.foreground),
        "an available label stays foreground"
    );
}

#[gpui::test]
fn picker_c2_narrow_path_strip_elides_its_start_and_keeps_the_leaf(cx: &mut TestAppContext) {
    let deep = "/srv/flea-sandbox/fn.a71HuDTp/arms/state.gpui/fixture/work/projects/archive/2026/october/Media";
    let mut f = fixture(
        cx,
        &format!(r#"{{"mode":"save","folder":"{deep}","name":"sa-d3-new.txt"{TEXT_FILES}}}"#),
        None,
    );
    f.cx.simulate_resize(size(px(720.), px(700.)));
    f.cx.run_until_parked();
    draw(&mut f);
    draw(&mut f);
    let path = f.read(|picker| picker.browse.path.clone());
    f.cx.update(|_, cx| {
        let view = path.read(cx).path();
        let view = view.read(cx);
        let crumbs = view.strips[0].crumbs();
        let line: String = crumbs.iter().map(|crumb| crumb.text.as_str()).collect();
        assert!(
            line.len() < deep.len(),
            "the 720 px strip is narrower than the path, so it elides: {line:?}"
        );
        assert!(
            line.starts_with('\u{2026}'),
            "the strip gives up its start, as PickerChrome.qml's ElideLeft does: {line:?}"
        );
        assert!(
            deep.ends_with(&line['\u{2026}'.len_utf8()..]),
            "what follows the ellipsis is the path's own tail, unbroken: {line:?}"
        );
        assert_eq!(
            crumbs.last().map(|crumb| crumb.text.as_str()),
            Some("Media")
        );
    });
}

fn fitted(path: &str, budget: usize) -> Vec<String> {
    use crate::path::crumbs::{crumbs, fit_start};
    fit_start(crumbs(path, "/home/u", None), budget)
        .into_iter()
        .map(|crumb| crumb.text)
        .collect()
}

#[test]
fn picker_c2_fit_start_gives_up_parents_from_the_left_and_never_the_leaf() {
    assert_eq!(
        fitted("/srv/aa/bbb/Media", 17),
        ["/", "srv/", "aa/", "bbb/", "Media"],
        "a path that fits is drawn whole"
    );
    assert_eq!(
        fitted("/srv/aa/bbb/Media", 12),
        ["\u{2026}a/", "bbb/", "Media"],
        "whole parents nearest the leaf stay, the next keeps its tail behind the ellipsis"
    );
    assert_eq!(
        fitted("/srv/aa/bbb/Media", 5),
        ["Media"],
        "a leaf that fills the strip is drawn alone, with no bare ellipsis before it"
    );
    let long = fitted("/srv/aa/an-unusually-long-leaf-name", 10);
    assert_eq!(
        long.len(),
        1,
        "the parents give way to a leaf too long: {long:?}"
    );
    assert!(
        long[0].starts_with("an-u") && long[0].ends_with("name") && long[0].contains('\u{2026}'),
        "the leaf keeps its head and tail around one ellipsis: {long:?}"
    );
}
