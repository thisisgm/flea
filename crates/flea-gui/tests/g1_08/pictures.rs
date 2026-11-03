//! Lent thumbnails: each draws aspect-fit in its slot at the size its decoded pixels call for, only on the row it was made from.
use super::frames::{frame, held, open, real_cx, rows, state, with};
use flea::protocol::RowFacts;
use flea_gui::grid::{
    element::{Clip, Decor, DropKind, GridState, Identity, PictureKind, Thumbnail},
    geometry::{fit_picture, Rect, ThumbSize},
    FocusSnapshot,
};
use gpui::{font, Image, ImageFormat, RenderImage, SvgRenderer};
use std::{collections::HashMap, path::PathBuf, process::Command, rc::Rc, sync::Arc};

/// A decoded picture of this size, built the way G2's worker result arrives: pixels already decoded, no file read here.
fn picture(width: u32, height: u32) -> Arc<RenderImage> {
    let mut bytes = format!("P6\n{width} {height}\n255\n").into_bytes();
    bytes.extend(std::iter::repeat_n([200u8, 120, 40], (width * height) as usize).flatten());
    Image::from_bytes(ImageFormat::Pnm, bytes)
        .to_image_data(SvgRenderer::new(Arc::new(())))
        .expect("a PNM decodes")
}

struct Lent {
    pictures: HashMap<usize, Arc<Thumbnail>>,
    clips: HashMap<usize, Clip>,
}

impl Decor for Lent {
    fn thumbnail(&self, listing: usize) -> Option<Arc<Thumbnail>> {
        self.pictures.get(&listing).cloned()
    }
    fn clip(&self, listing: usize) -> Option<Clip> {
        self.clips.get(&listing).cloned()
    }
}

fn lent(row: &RowFacts, kind: PictureKind, width: u32, height: u32) -> Arc<Thumbnail> {
    Arc::new(Thumbnail {
        identity: Identity::of(row),
        kind,
        image: picture(width, height),
    })
}

#[test]
fn fit_enlarges_small_shrinks_large_and_centres_on_whole_pixels() {
    let slot = Rect {
        x: 41.5,
        y: 14.,
        width: 64.,
        height: 64.,
    };
    let fit = |w, h| {
        let rect = fit_picture(slot, w, h);
        (rect.width, rect.height, rect.x - slot.x, rect.y - slot.y)
    };
    assert_eq!(
        fit(16, 12),
        (64., 48., 0., 8.),
        "a tiny landscape source fills the slot width"
    );
    assert_eq!(
        fit(48, 64),
        (48., 64., 8., 0.),
        "a portrait source fills the slot height"
    );
    assert_eq!(
        fit(64, 64),
        (64., 64., 0., 0.),
        "an exact source draws at its own size"
    );
    assert_eq!(
        fit(1024, 683),
        (64., 43., 0., 10.),
        "an oversized raw preview shrinks to the board's 64 by 43"
    );
    assert_eq!(fit(6000, 100), (64., 1., 0., 31.), "a sliver keeps a pixel");
    assert_eq!(
        fit(6000, 40),
        (64., 1., 0., 31.),
        "a sliver thinner than half a pixel still keeps one"
    );
    assert_eq!(
        fit(0, 10),
        (0., 0., 0., 0.),
        "an empty picture draws nothing"
    );
}

#[test]
fn every_drawn_picture_is_its_decoded_size_fit_to_the_slot_for_small_exact_and_oversized_sources() {
    let names = rows(12);
    // The first six are the ones the largest zoom draws: tiny, exact at 64, huge, tall, exact at 256 and small.
    let sources = [
        (4, 3),
        (64, 64),
        (4000, 3000),
        (683, 1024),
        (256, 256),
        (64, 48),
        (32, 32),
        (128, 96),
        (1024, 683),
        (48, 64),
        (1, 1),
        (300, 20),
    ];
    let kinds = [PictureKind::Image, PictureKind::Raw, PictureKind::Model];
    let pictures = sources
        .iter()
        .enumerate()
        .map(|(at, &(w, h))| (at, lent(&names[at], kinds[at % 3], w, h)))
        .collect();
    let mut cx = real_cx();
    let window = open(&mut cx, held(names));
    let grid = state(&mut cx, window);
    with(&mut cx, window, |host| {
        host.decor = Rc::new(Lent {
            pictures,
            clips: HashMap::new(),
        })
    });
    for (zoom, slot) in [("medium", 64.), ("xlarge", 128.), ("largest", 256.)] {
        with(&mut cx, window, |host| {
            host.view.thumb = ThumbSize::named(zoom)
        });
        frame(&mut cx, window);
        sized(&grid.borrow(), &sources, &kinds, slot);
    }
}

fn sized(state: &GridState, sources: &[(u32, u32)], kinds: &[PictureKind; 3], side: f32) {
    assert!(
        state.drawn().len() >= 6,
        "{} tiles drawn at {side}",
        state.drawn().len()
    );
    for (at, &(w, h)) in sources.iter().enumerate() {
        let Some(tile) = state.drawn_row(at) else {
            continue;
        };
        assert_eq!(
            (tile.slot.width, tile.slot.height),
            (side, side),
            "tile {at} slot at this zoom"
        );
        assert_eq!(
            tile.decoded,
            Some((w, h)),
            "tile {at} draws the decoded picture it was lent"
        );
        assert_eq!(
            tile.kind,
            Some(kinds[at % 3]),
            "tile {at} keeps the picture kind"
        );
        let drawn = tile.picture.expect("a picture box");
        assert_eq!(
            drawn,
            fit_picture(tile.slot, w, h),
            "tile {at} box against its decoded size"
        );
        assert!(
            drawn.width <= tile.slot.width && drawn.height <= tile.slot.height,
            "tile {at} never spills its slot"
        );
        assert!(
            drawn.width == tile.slot.width || drawn.height == tile.slot.height,
            "tile {at} fills one side of its slot"
        );
        let (aspect, box_aspect) = (w as f32 / h as f32, drawn.width / drawn.height);
        assert!(
            (aspect - box_aspect).abs() / aspect
                < 0.5 / drawn.width.min(drawn.height).max(1.) + 0.02,
            "tile {at} keeps its aspect {aspect} as {box_aspect}"
        );
        assert!(
            drawn.x.fract() == tile.slot.x.fract() && drawn.width.fract() == 0.,
            "tile {at} sits on whole pixels of its slot"
        );
    }
}

#[test]
fn a_picture_draws_only_on_the_row_it_was_made_from() {
    let names = rows(6);
    let mut stale = names[2].clone();
    stale.mtime += 60;
    let mut other = names[3].clone();
    other.name = "another.png".into();
    let pictures = HashMap::from([
        (1, lent(&names[1], PictureKind::Raw, 1024, 683)),
        (2, lent(&stale, PictureKind::Image, 64, 64)),
        (3, lent(&other, PictureKind::Model, 64, 64)),
    ]);
    let mut cx = real_cx();
    let window = open(&mut cx, held(names));
    let grid = state(&mut cx, window);
    with(&mut cx, window, |host| {
        host.decor = Rc::new(Lent {
            pictures,
            clips: HashMap::new(),
        })
    });
    frame(&mut cx, window);
    let state = grid.borrow();
    assert_eq!(
        state.drawn_row(1).unwrap().kind,
        Some(PictureKind::Raw),
        "the raw preview draws on its own row"
    );
    assert_eq!(
        state.drawn_row(2).unwrap().picture,
        None,
        "a picture of an older version of the row never draws"
    );
    assert_eq!(
        state.drawn_row(3).unwrap().picture,
        None,
        "a picture of another file at this index never draws"
    );
    assert_eq!(
        state.drawn_row(0).unwrap().picture,
        None,
        "a row with no picture keeps its mark"
    );
}

#[test]
fn a_clipped_tile_reserves_the_mark_strip_and_a_cut_tile_still_draws_its_picture() {
    let names = rows(6);
    let pictures = HashMap::from([(1, lent(&names[1], PictureKind::Image, 64, 48))]);
    let svg: &'static [u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2h20v20H2z"/></svg>"#;
    let clips = HashMap::from([
        (
            1,
            Clip {
                cut: true,
                key: "test-clip:cut".into(),
                svg,
            },
        ),
        (
            2,
            Clip {
                cut: false,
                key: "test-clip:copy".into(),
                svg,
            },
        ),
    ]);
    let mut cx = real_cx();
    let window = open(&mut cx, held(names));
    let grid = state(&mut cx, window);
    with(&mut cx, window, |host| {
        host.decor = Rc::new(Lent { pictures, clips })
    });
    frame(&mut cx, window);
    let state = grid.borrow();
    let (plain, cut, copied) = (
        state.drawn_row(0).unwrap(),
        state.drawn_row(1).unwrap(),
        state.drawn_row(2).unwrap(),
    );
    assert_eq!(
        plain.caption.width - cut.caption.width,
        9. + 12.,
        "a clip mark takes a gap and its own size off the label"
    );
    assert_eq!(copied.caption.width, cut.caption.width);
    assert!(cut.picture.is_some(), "a cut picture still draws, dimmed");
}

#[test]
fn a_typed_run_washes_both_caption_lines_and_the_cursor_tile_sets_it_bold_instead() {
    let mut names = rows(12);
    names[1].name = "my-vacation-1.jpeg".into();
    names[2].name = "my-vacation-2.jpeg".into();
    let mut cx = real_cx();
    let window = open(&mut cx, held(names));
    let grid = state(&mut cx, window);
    with(&mut cx, window, |host| {
        host.overlay.query = "N-".into();
        host.focus = Rc::new(FocusSnapshot {
            cursor: 2,
            ..FocusSnapshot::default()
        });
    });
    frame(&mut cx, window);
    let (one_line, line) = {
        let state = grid.borrow();
        let tile = state.drawn_row(1).unwrap();
        assert_eq!(tile.caption_text(), Some("my-vacation-\n1.jpeg"));
        let [first, second] = tile.washes;
        let first = first.expect("the run on the first line is washed");
        assert_eq!(
            (first.y, second),
            (0., None),
            "a run inside one line washes that line alone"
        );
        assert!(
            first.x > tile.caption.width / 2. && first.right() <= tile.caption.width,
            "the run sits at the end of its centred line"
        );
        assert_eq!(
            state.drawn_row(2).unwrap().washes,
            [None, None],
            "the cursor tile shows its run bold, never washed"
        );
        assert_eq!(
            state.drawn_row(3).unwrap().washes,
            [None, None],
            "a caption without the run is bare"
        );
        (first.width, state.layout.unwrap().metrics.caption_line)
    };
    with(&mut cx, window, |host| host.overlay.query = "n-1".into());
    frame(&mut cx, window);
    let state = grid.borrow();
    let tile = state.drawn_row(1).unwrap();
    let [first, second] = tile.washes;
    let (first, second) = (first.expect("first line"), second.expect("second line"));
    assert_eq!(
        (first.y, second.y),
        (0., line),
        "a run across the break washes the end of one line and the head of the next"
    );
    assert_eq!(
        first.width, one_line,
        "n- washes the same box on either query"
    );
    // Measured from the drawn frame so any face holds: the first wash ends the widest line, the second opens a narrower centred one.
    let widest_left = tile.painted.x - tile.caption.x;
    assert!(
        (first.right() - (widest_left + tile.painted.width)).abs() < 0.5,
        "the first wash ends where my-vacation- ends"
    );
    assert!(
        second.width > 0. && second.x > widest_left,
        "the second wash opens the shorter centred line 1.jpeg"
    );
    drop(state);
    with(&mut cx, window, |host| host.overlay.renaming = Some(1));
    frame(&mut cx, window);
    assert_eq!(
        grid.borrow().drawn_row(1).unwrap().washes,
        [None, None],
        "a tile under rename shows the editor, not the run"
    );
}

fn catalog_root() -> PathBuf {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g1-08");
    std::fs::create_dir_all(&parent).unwrap();
    let output = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("test.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), b"g1-08\n").unwrap();
    root
}

#[test]
fn drop_labels_come_from_the_catalog_with_english_fallback_and_follow_a_language_change() {
    let root = catalog_root();
    std::fs::create_dir_all(root.join("catalog/de")).unwrap();
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    std::fs::write(
        root.join("omarchy/default/i18n/locales.json"),
        r#"{"en":{},"de":{"name":"Deutsch"}}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("catalog/de/flea.json"),
        r#"{"grid.drop.move":"hierher verschieben"}"#,
    )
    .unwrap();
    std::fs::write(root.join("language"), "de\n").unwrap();
    let paths = flea::i18n::Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog")],
    };
    let mut catalog = flea::i18n::I18n::load(paths);
    let mut state = GridState::new(font("JetBrains Mono"));
    state.set_catalog(&catalog);
    let labels = |state: &GridState| {
        [DropKind::Move, DropKind::Copy, DropKind::Link].map(|kind| state.label(kind).to_owned())
    };
    assert_eq!(
        labels(&state),
        ["hierher verschieben", "copy here", "link here"],
        "a key the translation lacks falls back to English"
    );
    std::fs::write(root.join("language"), "en\n").unwrap();
    catalog.reload();
    state.set_catalog(&catalog);
    assert_eq!(
        labels(&state),
        ["move here", "copy here", "link here"],
        "a language change takes the new words"
    );
    assert_eq!(
        labels(&GridState::new(font("JetBrains Mono"))),
        ["", "", ""],
        "no catalog yet draws no label rather than a key"
    );
}
