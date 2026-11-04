//! CTL2 EY-596/597: the HIG order and the vertical frame clamp through the real Shell environment.
use super::{
    join_tests::open,
    menu::{Face, Menu},
};
use crate::list::Typeface;
use flea::ui_logic_mn1::{
    build::{listing_entries, Context},
    fit::{Metrics, Rect},
};
use gpui::{px, TestAppContext};

fn rows() -> Vec<flea::ui_logic_mn1::Entry> {
    listing_entries(&Context {
        row_mode: 0o100644,
        row_is_file: true,
        row_name: "notes.txt".into(),
        printable: true,
        hidden: [
            "run",
            "openAsText",
            "makeExecutable",
            "shelf",
            "runScript",
            "compare",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        archive_formats: vec!["zip".into()],
        taildrop_installed: true,
        localsend_installed: true,
        dropbox_installed: true,
        ..Context::default()
    })
}

#[test]
fn sol_menu_order2_ey596_hig_order_is_already_fixed() {
    let rows = rows();
    let groups: Vec<Vec<&str>> = rows
        .split(|row| row.separator)
        .map(|group| group.iter().map(|row| row.action.as_ref()).collect())
        .collect();
    assert_eq!(
        groups,
        [
            vec!["open", "openWith", "print"],
            vec!["cut", "copy", "paste", "duplicate", "rename"],
            vec!["compress"],
            vec!["taildrop", "localsend", "dropbox"],
            vec![
                "openTerminal",
                "moveTo",
                "copyTo",
                "properties",
                "permissions",
                "copyAs",
                "tags"
            ],
            vec!["toggleHidden", "toggleLangMarks"],
            vec!["trash", "deletePermanently"],
        ],
        "menu-past-fold draws the cited rows; Finder 8.1 does not override this HIG order"
    );
}

#[gpui::test]
fn sol_menu_order2_ey597_long_menu_clamps_vertically_without_changing_width(
    cx: &mut TestAppContext,
) {
    let fixture = open(cx);
    fixture
        .handle
        .update(cx, |root, window, cx| {
            let typeface = Typeface::omarchy_default();
            assert_eq!(typeface.body, px(14.));
            let env = root.shell.read(cx).menu_env(&typeface);
            let work = Rect {
                x: 0.,
                y: 0.,
                w: 1100.,
                h: 700.,
            };
            assert_eq!(
                Metrics::at(14.).place((206., 300.), 257., 1000, work, env.inset),
                Rect {
                    x: 206.,
                    y: 5.,
                    w: 257.,
                    h: 690.
                },
                "the maximum frame has 5 pixels above and below"
            );
            let mut menu = Menu::default();
            menu.open(Face::File, rows(), (206., 300.), work, "item", &env, window);
            let frame = menu.card.as_ref().unwrap().frame;
            assert_eq!((frame.x, frame.y, frame.w), (206., 5., 257.));
            assert!(
                frame.y + frame.h <= 695.,
                "the fold cue stays inside the bottom inset"
            );
            menu.refresh(rows(), &env, window);
            let frame = menu.card.as_ref().unwrap().frame;
            assert_eq!((frame.x, frame.y, frame.w), (206., 5., 257.));
            assert!(
                frame.y + frame.h <= 695.,
                "refresh uses the same vertical clamp"
            );
        })
        .unwrap();
}

#[gpui::test]
fn sol_menu_order2_ey597_flyout_preserves_horizontal_slide(cx: &mut TestAppContext) {
    let fixture = open(cx);
    fixture
        .handle
        .update(cx, |root, window, cx| {
            let typeface = Typeface::omarchy_default();
            let env = root.shell.read(cx).menu_env(&typeface);
            let mut menu = Menu::default();
            let work = Rect {
                x: 0.,
                y: 0.,
                w: 480.,
                h: 240.,
            };
            menu.open(Face::File, rows(), (470., 200.), work, "item", &env, window);
            menu.open_submenu_for("copyAs", false, &env, window);
            let frame = menu.flyout.as_ref().unwrap().card.frame;
            assert_eq!(
                (frame.x, frame.w),
                (219., 257.),
                "the existing horizontal slide and width stay fixed"
            );
            assert!(
                frame.y >= 5. && frame.y + frame.h <= 235.,
                "the flyout uses the 5 pixel vertical inset"
            );
        })
        .unwrap();
}
