//! GPUI tests: path bar, rail, tiles, chip menu, header.
use super::chrome::{ChipMenu, PathBar, RailPanel, TileGrid};
use super::listing::Listing;
use super::view::ListingView;
use super::view_tests::{english, listed, rows};
use crate::list::pane::Held;
use crate::list::Typeface;
use crate::path::view::Inputs;
use crate::rail::Mode;
use flea::ui_logic_picker_listing::request::Chip;
use gpui::{px, size, SharedString, TestAppContext};
use std::rc::Rc;

fn held_window() -> Rc<Held> {
    let mut listing = Listing::default();
    listing.open("/big");
    let gen = listing.generation();
    assert!(listing.receive(gen, &listed(100_000)));
    assert!(listing.receive(gen, &rows(gen)));
    listing.held()
}

#[gpui::test]
fn the_path_bar_draws_the_picker_path_it_is_given(cx: &mut TestAppContext) {
    let (bar, cx) = cx.add_window_view(|_, cx| PathBar::new(&english(), cx));
    cx.simulate_resize(size(px(800.), px(120.)));
    bar.update(cx, |bar, cx| {
        let inputs = Inputs {
            paths: [
                SharedString::from("/home/gm/Documents"),
                SharedString::default(),
            ],
            home: "/home/gm".into(),
            ..Inputs::default()
        };
        bar.sync(inputs, cx);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("pk2-path").is_some(),
        "the path bar is drawn"
    );
    bar.update(cx, |bar, cx| {
        let path = bar.path().read(cx);
        assert_eq!(path.inputs.paths[0].as_ref(), "/home/gm/Documents");
        assert!(
            path.paint.borrow().area.is_some(),
            "the PathView painted its chrome area in the frame"
        );
    });
}

#[gpui::test]
fn the_places_rail_builds_its_content_once_and_draws_it(cx: &mut TestAppContext) {
    let (rail, cx) = cx.add_window_view(|_, cx| RailPanel::new(Mode::Docked, 220., cx));
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("pk2-rail").is_some(), "the rail is drawn");
    assert!(
        cx.debug_bounds("pk2-places").is_some(),
        "the places content is drawn inside it"
    );
    rail.update(cx, |rail, cx| {
        assert_eq!(rail.host().read(cx).constructed, 1)
    });
}

#[gpui::test]
fn the_tile_grid_draws_the_held_window_of_a_hundred_thousand_row_folder(cx: &mut TestAppContext) {
    let (tiles, cx) = cx.add_window_view(|_, _| TileGrid::new((800., 600.)));
    cx.simulate_resize(size(px(800.), px(600.)));
    let held = held_window();
    tiles.update(cx, |tiles, cx| {
        tiles.show(held);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("pk2-tiles").is_some(),
        "the tile grid is drawn over the held rows"
    );
    tiles.update(cx, |tiles, _| {
        let (hits, misses, _) = tiles.shaped();
        assert!(
            misses > 0,
            "the grid shaped the held window's tiles in the frame (hits {hits})"
        );
        assert!(tiles.frames() > 0, "the grid painted a frame");
    });
}

#[gpui::test]
fn the_chip_menu_opens_over_every_chip_then_all_files(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (menu, cx) =
        cx.add_window_view(|_, _| ChipMenu::new(english(), Typeface::omarchy_default()));
    cx.simulate_resize(size(px(800.), px(600.)));
    let chips = vec![
        Chip {
            label: "Archives".into(),
            index: 0,
        },
        Chip {
            label: "All files".into(),
            index: -1,
        },
    ];
    menu.update_in(cx, |menu, window, cx| {
        menu.open(&chips, (40., 40.), window);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(
        menu.update(cx, |menu, _| menu.opened()),
        "the card opens over the chips"
    );
    assert!(
        cx.debug_bounds("pk2-chip-menu").is_some(),
        "the chip menu card is drawn"
    );
    let quads = menu.update_in(cx, |_, window, _| window.painted_quads().len());
    assert!(
        quads > 0,
        "the MenuElement painted the card's quads in the frame"
    );
    menu.update(cx, |menu, _| {
        assert_eq!(
            menu.card_labels(),
            vec!["Archives", "All files"],
            "the card lists every chip then All files"
        );
    });
}

// mn1 fits every card inside its work area.
#[gpui::test]
fn the_chip_card_stays_inside_the_window_it_opens_in(cx: &mut TestAppContext) {
    let (menu, cx) =
        cx.add_window_view(|_, _| ChipMenu::new(english(), Typeface::omarchy_default()));
    cx.simulate_resize(size(px(800.), px(600.)));
    let chips = vec![
        Chip {
            label: "Archives".into(),
            index: 0,
        },
        Chip {
            label: "All files".into(),
            index: -1,
        },
    ];
    menu.update_in(cx, |menu, window, _| {
        menu.open(&chips, (700., 560.), window);
    });
    cx.run_until_parked();
    menu.update(cx, |menu, _| {
        let frame = menu.card_frame().expect("the card opened");
        assert!(
            frame.x + frame.w <= 800. && frame.y + frame.h <= 600.,
            "the card {frame:?} runs past the 800x600 window it opened in"
        );
    });
}

#[gpui::test]
fn the_listing_header_is_drawn_above_the_held_rows(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|_, _| ListingView::new(english(), Typeface::omarchy_default()));
    cx.simulate_resize(size(px(800.), px(600.)));
    view.update(cx, |view, cx| {
        view.open("/big");
        let gen = view.generation();
        assert!(view.receive(gen, &listed(100_000)));
        assert!(view.receive(gen, &rows(gen)));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("pk2-header").is_some(),
        "the column header is drawn"
    );
    assert!(
        cx.debug_bounds("pk2-listing").is_some(),
        "and the held rows under it"
    );
    view.update(cx, |view, _| {
        assert!(
            view.header_frames() > 0,
            "the HeaderElement shaped and painted its titles in the frame"
        );
        assert_eq!(
            view.shaped_rows(),
            2,
            "the held rows shaped under the header"
        );
    });
}
