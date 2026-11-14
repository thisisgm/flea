use super::*;
use flea::protocol::{Reply, RowFacts};

fn pair(nav: &mut Navigator, pane: &mut Pane, listing: usize) {
    let before = pane.path().to_owned();
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
        nav.after_reply(pane, &before, &mut Vec::new());
    }
}

#[test]
fn m9_g1rows_05_forget_joins_the_old_sort_and_default_list_to_their_own_rows() {
    for listed in [false, true] {
        let data = flea::uistate::from_file("{}");
        let mut nav = Navigator::new(
            Rc::new(RefCell::new(Folders::from_settings(&data, Vec::new()))),
            Rc::default(),
        );
        let mut pane = Pane::new("/A".into(), 620.);
        nav.listing(&pane, "/A", false);
        pair(&mut nav, &mut pane, 1);
        let size = Order {
            key: "size",
            desc: true,
        };
        assert_eq!(
            nav.resort(&mut pane, size, &mut Vec::new(), String::new()),
            Ok(true)
        );
        nav.take();
        if listed {
            pane.receive(Reply::Listed {
                path: "/A".into(),
                total: 1,
            })
            .unwrap();
            nav.after_reply(&mut pane, "/A", &mut Vec::new());
        }
        assert_eq!(nav.forget_sort(&mut pane, String::new()), Ok(true));
        if listed {
            assert!(!pane
                .receive(Reply::Rows {
                    start: 0,
                    rows: vec![RowFacts::default()],
                    listing: 2
                })
                .unwrap());
            nav.after_reply(&mut pane, "/A", &mut Vec::new());
            assert_eq!(nav.drawn_order(), Order::default());
        } else {
            pair(&mut nav, &mut pane, 2);
            assert_eq!(
                nav.drawn_order(),
                size,
                "the earlier sort's rows arrive before the default list"
            );
        }
        assert_eq!(nav.sort_snapshot(&pane), Err(Refusal::Busy));
        pair(&mut nav, &mut pane, 3);
        assert_eq!(nav.drawn_order(), Order::default());
        assert_eq!(nav.order, Order::default());
        assert_eq!(nav.sort_snapshot(&pane).unwrap().order, Order::default());
    }
}
