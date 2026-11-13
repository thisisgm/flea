//! A navigation the pane asked for: the Columns area records what it leaves before the listing changes.
use super::with_area;
use crate::{
    columns::{
        carry::{Leaving, ListOrder},
        peeks::Facts,
    },
    navigation::Listing,
    window::pane::PaneHost,
};

pub(crate) fn requested(host: &mut PaneHost, listing: &Listing) {
    let now = host.pane.now();
    let order = host.nav.order;
    let folders = host.nav.folders.borrow();
    let (folders_first, group_by_kind) = (folders.folders_first, folders.group_by_kind);
    drop(folders);
    let held = host.pane.held.clone();
    let narrowed =
        held.indexed() || host.pane.state.search || !host.list.borrow().view.query.is_empty();
    with_area(host, |columns, pane, _| {
        columns.area.listing_requested(
            Leaving {
                here: pane.lens.here,
                target: &listing.path,
                tag: "",
                window_size: pane.lens.first,
                total: held.total,
                held_start: held.start,
                narrowed,
                rows: &held.rows,
                order: ListOrder {
                    by: order.key,
                    desc: order.desc,
                    folders_first,
                    group_by_kind,
                },
                facts: columns.area.peeks.source(pane.lens.here).map_or(
                    Facts {
                        dev: 0,
                        writable: true,
                    },
                    |source| Facts {
                        dev: source.dev,
                        writable: source.writable,
                    },
                ),
            },
            &pane.lens,
            now,
        );
        columns.seen = None;
    });
}
