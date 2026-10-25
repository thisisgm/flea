use flea::{
    jsondoc::Json,
    protocol::{Reply, RowFacts},
};
use flea_gui::{
    navigation::{
        folder_state::{Folders, Order, Writer},
        sort::Snapshot,
        Command, Navigator, Refusal,
    },
    pane::Pane,
};
use std::{cell::RefCell, rc::Rc};

const PANE_HEIGHT: f32 = 620.;
// The watch clock reading at which a change seen at 0 fires its reread.
const WATCH_DELAY_MS: u64 = 400;

fn order(key: &'static str, desc: bool) -> Order {
    Order { key, desc }
}

struct Tab {
    pane: Pane,
    nav: Navigator,
    selected: Vec<usize>,
    generation: usize,
}

impl Tab {
    fn new(settings: &str, path: &str) -> Self {
        let data = flea::uistate::from_file(settings);
        Self::shared(
            Rc::new(RefCell::new(Folders::from_settings(&data, Vec::new()))),
            Rc::default(),
            path,
        )
    }

    fn shared(folders: Rc<RefCell<Folders>>, writer: Rc<RefCell<Writer>>, path: &str) -> Self {
        let mut tab = Self {
            pane: Pane::new(path.into(), PANE_HEIGHT),
            nav: Navigator::new(folders, writer),
            selected: Vec::new(),
            generation: 0,
        };
        tab.nav.listing(&tab.pane, path, false);
        tab.land(path);
        tab
    }

    fn land(&mut self, path: &str) {
        let before = self.pane.path().to_owned();
        let now = self.pane.now();
        self.generation += 1;
        self.pane
            .receive_at(
                Reply::Listed {
                    path: path.into(),
                    total: 3,
                },
                now,
            )
            .unwrap();
        assert!(self
            .pane
            .receive_at(
                Reply::Rows {
                    start: 0,
                    listing: self.generation,
                    rows: ["alpha.txt", "beta.png", "gamma.md"]
                        .map(|name| RowFacts {
                            name: name.into(),
                            ..Default::default()
                        })
                        .into(),
                },
                now
            )
            .unwrap());
        self.nav
            .after_reply(&mut self.pane, &before, &mut self.selected);
    }

    fn open(&mut self, path: &str) -> Order {
        assert_eq!(self.nav.open(&mut self.pane, path), Ok(true));
        let commands = self.nav.take();
        assert_eq!(commands.len(), 1, "browsing emits only the list");
        let Command::List(listing) = &commands[0] else {
            panic!("missing list")
        };
        assert_eq!(listing.path, path);
        let result = listing.order;
        self.land(path);
        assert_eq!(self.nav.order, result);
        result
    }

    fn choose(&mut self, order: Order) -> Vec<Command> {
        assert_eq!(
            self.nav
                .resort(&mut self.pane, order, &mut self.selected, String::new()),
            Ok(true)
        );
        let commands = self.nav.take();
        let path = self.pane.path().to_owned();
        self.land(&path);
        commands
    }
}

#[test]
fn m9_g1rows_05_browsing_never_writes_sort() {
    for key in ["name", "size", "mtime", "kind"] {
        for desc in [false, true] {
            let own = order(key, desc);
            let mut tab = Tab::new(r#"{"sort":{"key":"size","reverse":true}}"#, "/A");
            if tab.nav.order == own {
                tab.choose(order("name", true));
            }
            let commands = tab.choose(own);
            assert!(commands.iter().any(|c| matches!(c, Command::Persist(_))));
            let writes = tab.nav.writer.borrow().sent;
            assert_eq!(tab.open("/B"), order("size", true));
            assert_eq!(tab.open("/A"), own);
            assert!(tab.nav.changed(&tab.pane, "/A", 0));
            assert!(tab
                .nav
                .fire_watch(&mut tab.pane, &tab.selected, WATCH_DELAY_MS));
            assert!(
                matches!(&tab.nav.take()[..], [Command::List(listing)] if listing.order == own)
            );
            tab.land("/A");
            assert_eq!(tab.nav.writer.borrow().sent, writes);
        }
    }
}

#[test]
fn m9_g1rows_05_transfer_restores_tab_order_until_navigation() {
    let mut first = Tab::new("{}", "/A");
    first.choose(order("size", true));
    let mut second = Tab::shared(first.nav.folders.clone(), first.nav.writer.clone(), "/A");
    second.choose(order("kind", false));
    assert_eq!(first.nav.order, order("size", true));
    assert_eq!(second.nav.order, order("kind", false));
    let snapshot = first.nav.sort_snapshot(&first.pane).unwrap();
    assert_eq!(
        snapshot,
        Snapshot {
            path: "/A".into(),
            order: order("size", true)
        }
    );
    let mut receiver = Tab::new(
        r#"{"folderSorts":{"/A":{"key":"date","reverse":false}}}"#,
        "/B",
    );
    let before = receiver.nav.folders.borrow().get("/A");
    assert_eq!(
        receiver
            .nav
            .restore_sort_snapshot(&mut receiver.pane, &snapshot),
        Ok(true)
    );
    assert!(
        matches!(&receiver.nav.take()[..], [Command::List(listing)] if listing.order == snapshot.order)
    );
    assert_eq!(
        receiver.nav.sort_snapshot(&receiver.pane),
        Err(Refusal::Busy)
    );
    receiver.land("/A");
    assert_eq!(receiver.nav.sort_snapshot(&receiver.pane), Ok(snapshot));
    assert_eq!(receiver.nav.writer.borrow().sent, 0);
    assert_eq!(receiver.nav.folders.borrow().get("/A"), before);
    assert!(first.nav.changed(&first.pane, "/A", 0));
    assert!(first
        .nav
        .fire_watch(&mut first.pane, &first.selected, WATCH_DELAY_MS));
    let tab_refresh = first
        .nav
        .take()
        .into_iter()
        .find_map(|command| match command {
            Command::List(listing) => Some(listing.order),
            _ => None,
        });
    assert!(receiver.nav.changed(&receiver.pane, "/A", 0));
    assert!(receiver
        .nav
        .fire_watch(&mut receiver.pane, &receiver.selected, WATCH_DELAY_MS));
    let transfer_refresh = receiver
        .nav
        .take()
        .into_iter()
        .find_map(|command| match command {
            Command::List(listing) => Some(listing.order),
            _ => None,
        });
    receiver.land("/A");
    assert_eq!(receiver.open("/B"), Order::default());
    assert_eq!(receiver.open("/A"), order("mtime", false));
    assert_eq!(
        [tab_refresh, transfer_refresh],
        [Some(order("size", true)); 2],
        "a watched re-read must keep each tab's order until it changes folder"
    );
}

#[test]
fn m9_g1rows_05_forget_and_off_restore_global_sort() {
    let data = r#"{"sort":{"key":"date","reverse":true},"folderSorts":{"/A":{"key":"size","reverse":true}}}"#;
    let mut tab = Tab::new(data, "/A");
    assert_eq!(tab.nav.forget_sort(&mut tab.pane, String::new()), Ok(true));
    let commands = tab.nav.take();
    assert!(
        matches!(&commands[..], [Command::Persist(_), Command::List(listing)] if listing.order == order("mtime", true))
    );
    assert_eq!(tab.nav.folders.borrow().get("/A"), None);
    tab.land("/A");
    assert_eq!(tab.nav.order, order("mtime", true));
    let mut off = Tab::new(
        &data.replace("{\"sort\"", "{\"rememberSort\":false,\"sort\""),
        "/A",
    );
    assert_eq!(off.nav.order, order("mtime", true));
    off.choose(order("kind", false));
    assert_eq!(off.nav.writer.borrow().sent, 0);
    assert_eq!(off.open("/B"), order("mtime", true));
    assert_eq!(off.open("/A"), order("mtime", true));
    off.nav.forget_sort(&mut off.pane, String::new()).unwrap();
    assert_eq!(off.nav.writer.borrow().sent, 0);
    assert_eq!(
        off.nav.folders.borrow().get("/A"),
        Some(order("size", true))
    );
}

#[test]
fn m9_g1rows_05_501st_choice_evicts_and_restart_uses_persisted_orders() {
    let mut document = flea::uistate::from_file("{}");
    let mut tab = Tab::new("{}", "/start");
    for i in 0..501 {
        let path = format!("/folder-{i}");
        tab.open(&path);
        let own = order(["size", "mtime", "kind"][i % 3], i % 2 == 0);
        for command in tab.choose(own) {
            if let Command::Persist(patch) = command {
                document =
                    flea::uistate::patched(&document, &flea::jsondoc::parse(&patch).unwrap())
                        .unwrap();
            }
        }
    }
    let restored = Folders::from_settings(&document, Vec::new());
    assert_eq!(restored.sort_count(), 500);
    assert_eq!(tab.nav.folders.borrow().sort_count(), 500);
    assert_eq!(restored.get("/folder-0"), None);
    for i in 1..501 {
        let path = format!("/folder-{i}");
        assert_eq!(restored.get(&path), tab.nav.folders.borrow().get(&path));
    }
    assert_eq!(tab.nav.writer.borrow().sent, 501);
    let mut restarted = Tab::shared(Rc::new(RefCell::new(restored)), Rc::default(), "/start");
    assert_eq!(restarted.open("/folder-500"), order("kind", true));
    assert_eq!(restarted.open("/folder-0"), Order::default());
    assert_eq!(restarted.nav.writer.borrow().sent, 0);
    assert_eq!(
        document
            .get("folderSorts")
            .and_then(Json::as_object)
            .unwrap()
            .len(),
        500
    );
}

#[test]
fn m9_g1rows_05_late_list_reply_cannot_replace_the_restored_folder() {
    let mut tab = Tab::new(
        r#"{"folderSorts":{"/A":{"key":"size","reverse":true},"/B":{"key":"kind","reverse":false}}}"#,
        "/start",
    );
    tab.nav.open(&mut tab.pane, "/A").unwrap();
    tab.nav.open(&mut tab.pane, "/B").unwrap();
    tab.nav.take();
    let now = tab.pane.now();
    tab.pane
        .receive_at(
            Reply::Listed {
                path: "/A".into(),
                total: 1,
            },
            now,
        )
        .unwrap();
    assert!(!tab
        .pane
        .receive_at(
            Reply::Rows {
                start: 0,
                listing: 2,
                rows: vec![RowFacts::default()]
            },
            now
        )
        .unwrap());
    assert_eq!(tab.pane.path(), "/start");
    tab.generation = 2;
    tab.land("/B");
    assert_eq!(tab.nav.order, order("kind", false));
    assert_eq!(tab.pane.path(), "/B");
    assert_eq!(tab.nav.writer.borrow().sent, 0);
}

#[test]
fn m9_g1rows_05_snapshot_refuses_unsettled_or_search_rows_and_invalid_orders() {
    let mut tab = Tab::new("{}", "/A");
    tab.nav
        .sort_next(&mut tab.pane, &mut tab.selected, String::new())
        .unwrap();
    assert_eq!(tab.nav.sort_snapshot(&tab.pane), Err(Refusal::Busy));
    tab.nav.take();
    tab.land("/A");
    tab.pane.state.search = true;
    assert_eq!(tab.nav.sort_snapshot(&tab.pane), Err(Refusal::NotLoaded));
    tab.pane.state.search = false;
    let before = tab.nav.order;
    let invalid = Snapshot {
        path: "/B".into(),
        order: order("mode", false),
    };
    assert_eq!(
        tab.nav.restore_sort_snapshot(&mut tab.pane, &invalid),
        Err(Refusal::NotLoaded)
    );
    assert_eq!(tab.nav.order, before);
    assert!(tab.nav.take().is_empty());
}

#[test]
fn m9_g1rows_05_forgetting_while_busy_changes_no_preference() {
    let mut tab = Tab::new(
        r#"{"folderSorts":{"/A":{"key":"size","reverse":true}}}"#,
        "/A",
    );
    tab.nav.open(&mut tab.pane, "/B").unwrap();
    tab.nav.take();
    assert_eq!(
        tab.nav.forget_sort(&mut tab.pane, String::new()),
        Err(Refusal::Busy)
    );
    assert_eq!(
        tab.nav.folders.borrow().get("/A"),
        Some(order("size", true))
    );
    assert_eq!(tab.nav.writer.borrow().sent, 0);
    assert!(tab.nav.take().is_empty());
}

#[test]
fn m9_g1rows_05_forgetting_with_remember_off_preserves_saved_map() {
    let mut tab = Tab::new(
        r#"{"rememberSort":false,"folderSorts":{"/A":{"key":"size","reverse":true}}}"#,
        "/A",
    );
    assert_eq!(tab.nav.forget_sort(&mut tab.pane, String::new()), Ok(true));
    assert_eq!(
        tab.nav.folders.borrow().get("/A"),
        Some(order("size", true))
    );
    assert_eq!(tab.nav.writer.borrow().sent, 0);
    assert!(
        matches!(&tab.nav.take()[..], [Command::List(listing)] if listing.order == Order::default())
    );
}
