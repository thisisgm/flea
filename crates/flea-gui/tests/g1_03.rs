use flea::protocol::{Reply, RowFacts};
use flea_gui::navigation::{
    folder_state::{Folders, Writer},
    Command, Navigator,
};
use flea_gui::pane::Pane;
use std::{cell::RefCell, rc::Rc};

pub fn named(index: usize) -> String {
    format!("file-{index:05}")
}

pub fn rows(start: usize, count: usize, name: impl Fn(usize) -> String) -> Vec<RowFacts> {
    (start..start + count)
        .map(|index| RowFacts {
            name: name(index),
            ..RowFacts::default()
        })
        .collect()
}

pub fn navigator(settings: &str) -> Navigator {
    let folders = Folders::from_settings(&flea::jsondoc::parse(settings).unwrap(), Vec::new());
    Navigator::new(
        Rc::new(RefCell::new(folders)),
        Rc::new(RefCell::new(Writer::default())),
    )
}

// One listed+rows pair through the pane, then the navigator's landing, as the shell runs them.
pub fn land(
    pane: &mut Pane,
    nav: &mut Navigator,
    selected: &mut Vec<usize>,
    (path, total, start, listing): (&str, usize, usize, usize),
    name: impl Fn(usize) -> String,
) -> bool {
    let before = pane.path().to_owned();
    let generation = pane.held.listing;
    let now = pane.now();
    pane.receive_at(
        Reply::Listed {
            path: path.into(),
            total,
        },
        now,
    )
    .unwrap();
    let count = total.saturating_sub(start).min(pane.first_count());
    let committed = pane
        .receive_at(
            Reply::Rows {
                start,
                rows: rows(start, count, name),
                listing,
            },
            now,
        )
        .unwrap();
    // PaneHost::changed clears row-indexed marks when the numbering moves, before the landing.
    if pane.held.listing != generation {
        selected.clear();
    }
    nav.after_reply(pane, &before, selected);
    committed
}

pub fn settled(path: &str, total: usize) -> (Pane, Navigator, Vec<usize>) {
    let mut pane = Pane::new(path.into(), 620.);
    let mut nav = navigator("{}");
    let mut selected = Vec::new();
    assert!(land(
        &mut pane,
        &mut nav,
        &mut selected,
        (path, total, 0, 1),
        named
    ));
    (pane, nav, selected)
}

pub fn lists(commands: &[Command]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|command| match command {
            Command::List(listing) => Some(listing.path.clone()),
            _ => None,
        })
        .collect()
}

// The count of the first window a command batch asks for.
pub fn window_count(nav: &mut Navigator) -> usize {
    nav.take()
        .iter()
        .find_map(|command| match command {
            Command::Window { count, .. } => Some(*count),
            _ => None,
        })
        .unwrap()
}

#[path = "g1_03/anchor.rs"]
mod anchor;
#[path = "g1_03/folders.rs"]
mod folders;
#[path = "g1_03/history.rs"]
mod history;
#[path = "g1_03/sort.rs"]
mod sort;
#[path = "g1_03/sort_window.rs"]
mod sort_window;
#[path = "g1_03/transport.rs"]
mod transport;
#[path = "g1_03/watch.rs"]
mod watch;
