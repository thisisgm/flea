use super::*;

#[test]
fn pb_rest_inline_sizes_answer_their_absolute_rows_only() {
    let mut sizes = DirSizes::default();
    let rows = vec![
        RowFacts::default(),
        RowFacts {
            inline: Some(96),
            ..Default::default()
        },
    ];
    sizes.inlined(&rows, 10, 240);
    assert_eq!(sizes.size_for(10), None);
    assert_eq!(
        sizes.size_for(11),
        Some(Size {
            bytes: 96,
            partial: false
        })
    );
}

// A tab's own order, applied at once or after a held edit, never replaces the folder's memory or commits a fallback.
#[test]
fn b16_a_restored_tab_order_remembers_nothing_while_a_user_sort_does() {
    use flea::protocol::Reply;
    use std::{cell::RefCell, rc::Rc};
    for remember in [true, false] {
        let data = flea::uistate::from_file(&format!(r#"{{"rememberSort":{remember}}}"#));
        let folders = Rc::new(RefCell::new(
            super::super::folder_state::Folders::from_settings(&data, Vec::new()),
        ));
        let mut nav = super::super::Navigator::new(folders.clone(), Rc::default());
        let mut pane = crate::pane::Pane::new("/A".into(), 620.);
        let land = |nav: &mut super::super::Navigator, pane: &mut crate::pane::Pane, listing| {
            for reply in [
                Reply::Listed {
                    path: "/A".into(),
                    total: 1,
                },
                Reply::Rows {
                    start: 0,
                    rows: vec![RowFacts::default()],
                    listing,
                },
            ] {
                pane.receive(reply).unwrap();
                nav.after_reply(pane, "/A", &mut Vec::new());
            }
        };
        nav.listing(&pane, "/A", false);
        land(&mut nav, &mut pane, 1);
        let desc = Order {
            key: "name",
            desc: true,
        };
        assert_eq!(
            nav.resort(&mut pane, desc, &mut Vec::new(), String::new()),
            Ok(true)
        );
        land(&mut nav, &mut pane, 2);
        let writes = nav.writer.borrow().sent;
        assert_eq!(writes, usize::from(remember), "remember={remember}");
        assert_eq!(nav.committed_sort.take(), Some(desc), "remember={remember}");
        assert_eq!(folders.borrow().get("/A"), remember.then_some(desc));

        let asc = Order::default();
        assert_eq!(nav.restore_order(&mut pane, asc, &mut Vec::new()), Ok(true));
        land(&mut nav, &mut pane, 3);
        assert_eq!(nav.drawn_order(), asc);
        nav.gate.editing = true;
        assert_eq!(
            nav.restore_order(&mut pane, desc, &mut Vec::new()),
            Ok(false)
        );
        assert_eq!(nav.pending_sort, Some(Request::Restore(desc)));
        nav.gate.editing = false;
        assert_eq!(
            nav.apply_pending_sort(&mut pane, &mut Vec::new(), String::new()),
            Ok(true)
        );
        land(&mut nav, &mut pane, 4);
        assert_eq!(nav.drawn_order(), desc);
        assert_eq!(nav.writer.borrow().sent, writes, "remember={remember}");
        assert_eq!(nav.committed_sort, None, "remember={remember}");
        assert_eq!(folders.borrow().get("/A"), remember.then_some(desc));
    }
}
