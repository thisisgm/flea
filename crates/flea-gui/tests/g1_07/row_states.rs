//! Columns keep their board rungs with hover off by default or explicitly enabled.
use super::board::{alpha, palette};
use flea::jsondoc::parse;
use flea_gui::columns::geometry::Slot;
use flea_gui::columns::paint::{Clip, Focus, EDGE, HOVER, MARKED, SELECTION, WASH};
use flea_gui::list::hover_wash::HoverWash;
use std::rc::Rc;

#[test]
fn each_row_state_paints_the_board_plate_and_inks() {
    let shipped = HoverWash::read(&parse(flea::uischema::DEFAULTS).unwrap()).0;
    assert!(
        !shipped,
        "0401-settings-hover: a fresh install washes nothing under the pointer"
    );
    for hover_wash in [shipped, true] {
        let p = palette();
        let focused = Focus {
            cursor: Some(1),
            grey: false,
            selected: Rc::from([3usize]),
            hover: Some((Slot::Child, 2)),
            hover_wash,
            ..Default::default()
        };
        // Ancestor at rest: no plate, muted name, muted-frame mark.
        assert_eq!(focused.plate(&p, Slot::Parent, 4, false), None);
        let rest = p.row_ink(Clip::None, false, false, true);
        assert_eq!(
            (rest.name, rest.mark),
            (p.muted, p.muted_frame),
            "columns.html Code: --muted name, --muted-frame glyph"
        );
        // Ancestor lifted: the 8% hover plate, foreground name, muted-frame mark and chevron.
        assert_eq!(
            focused.plate(&p, Slot::Parent, 0, true),
            Some(alpha(p.foreground, HOVER))
        );
        let lifted = p.row_ink(Clip::None, false, true, true);
        assert_eq!(
            (lifted.name, lifted.mark, lifted.chevron),
            (p.foreground, p.muted_frame, p.muted_frame),
            "columns.html A"
        );
        // Active at rest: foreground name and size, muted-frame mark.
        assert_eq!(focused.plate(&p, Slot::Active, 0, false), None);
        let plain = p.row_ink(Clip::None, false, false, false);
        assert_eq!(
            (plain.name, plain.mark),
            (p.foreground, p.muted_frame),
            "columns.html empty, 40 B"
        );
        // Cursor with the keyboard: the 18% accent wash, the 2 px accent-frame edge, foreground name, mark and chevron.
        assert_eq!(
            focused.plate(&p, Slot::Active, 1, false),
            Some(p.accent.opacity(WASH))
        );
        assert!(focused.edge(Slot::Active, 1) && EDGE == 2.);
        assert_eq!(
            p.edge(),
            p.accent_frame,
            "the cursor edge draws in the accent frame"
        );
        let cursor = p.row_ink(Clip::None, true, false, false);
        assert_eq!(
            (cursor.name, cursor.mark, cursor.chevron),
            (p.foreground, p.foreground, p.foreground),
            "columns.html full"
        );
        // Cursor without the keyboard: the grey 18% rung, no edge.
        let grey = Focus {
            grey: true,
            ..focused.clone()
        };
        assert_eq!(
            grey.plate(&p, Slot::Active, 1, false),
            Some(alpha(p.foreground, SELECTION))
        );
        assert!(!grey.edge(Slot::Active, 1));
        // Marks keep the 35% plate; only the optional hover wash lifts a peek row.
        assert_eq!(
            focused.plate(&p, Slot::Active, 3, false),
            Some(alpha(p.foreground, MARKED))
        );
        assert_eq!(
            focused.plate(&p, Slot::Child, 2, false),
            hover_wash.then(|| alpha(p.foreground, HOVER))
        );
        assert_eq!(focused.plate(&p, Slot::Child, 1, false), None);
        assert!(!focused.edge(Slot::Parent, 1));
        // Peek rows: foreground name, muted-frame mark (columns.html f1.txt).
        let peek = p.row_ink(Clip::None, false, false, false);
        assert_eq!((peek.name, peek.mark), (p.foreground, p.muted_frame));
    }
}
