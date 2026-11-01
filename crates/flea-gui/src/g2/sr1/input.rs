//! Find's mounted platform adapter: the shared query input routed to this pane's search.
use super::Search;
use crate::{path::entry::Entry, state::Shell, strips::query_input};
use gpui::{Context, Window};
use std::{cell::RefCell, rc::Rc};

pub enum FindRoute {}

pub type QueryInput = query_input::QueryInput<FindRoute>;

impl query_input::Route for FindRoute {
    fn owns(shell: &Shell, side: usize, entry: &Rc<RefCell<Entry>>) -> bool {
        shell.panes[side].as_ref().is_some_and(|host| {
            host.search.owner.typing() && Rc::ptr_eq(&host.search.owner.entry, entry)
        })
    }

    fn commit(
        shell: &mut Shell,
        side: usize,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Shell>,
    ) {
        shell.search_update(
            side,
            None,
            |search: &mut Search, host| search.commit_query(generation, host),
            window,
            cx,
        );
    }

    fn redraw(_: &mut Shell, _: usize, cx: &mut Context<Shell>) {
        cx.notify();
    }
}
