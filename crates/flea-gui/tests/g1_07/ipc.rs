//! The flea.column* IPC reads over a real area: drawn tracks, rows on screen, counters, and preview reads left to their producer.
use super::check::Checks;
use super::fixture::{area, drain_asks, reply, row, Pane, Producers};
use flea_gui::columns::{peeks::Lens, preview_host::PreviewProducer};
use flea_gui::ipc::{columns::ColumnsAdapter, Adapter};

/// A preview producer that has ported exactly one read.
struct OneRead;

impl PreviewProducer for OneRead {
    fn ready(&self) -> bool {
        true
    }
    fn follow_selection(&mut self) {}
    fn arm_settle(&mut self) {}
    fn ask_cursor_thumb(&mut self) {}
    fn ipc(&self, function: &str, _: &[String]) -> Option<String> {
        (function == "columnFrameReady").then(|| "true".to_owned())
    }
}

fn call(adapter: &ColumnsAdapter<'_>, function: &str, argument: &str) -> Option<String> {
    let arguments: Vec<String> = if argument.is_empty() {
        Vec::new()
    } else {
        vec![argument.to_owned()]
    };
    adapter
        .call(function, &arguments)
        .map(|reply| reply.stdout.trim_end().to_owned())
}

#[test]
fn the_column_reads_answer_from_the_drawn_tracks() {
    let mut c = Checks::new("ui/Ipc.qml");
    let mut area = area();
    let mut producers = Producers::default();
    let pane = Pane::at("/srv/work", vec![row("A", true), row("notes.txt", false)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    let _ = drain_asks(&mut area);
    for (path, rows) in [
        ("/srv", vec![("work", true), ("other", true)]),
        ("/srv/work/A", vec![("f1", false), ("f2", false)]),
    ] {
        pane.with(|view| area.peeked(&reply(path, &rows), &view, &mut producers.ports(), 1));
    }
    let orders = |_: &str| None;
    let lens = Lens {
        here: "/srv/work",
        hidden: false,
        hidden_last: false,
        first: 512,
        orders: &orders,
    };
    let mut adapter = ColumnsAdapter {
        area: &area,
        lens,
        origin: (192., 27.),
        height: 600.,
        widths: "{\"size\":70}",
        child_empty: Some((false, 0., 0.)),
        preview: None,
    };
    c.eq(
        631,
        "columnCount is the drawn count",
        call(&adapter, "columnCount", ""),
        Some("3".into()),
    );
    c.eq(
        635,
        "columnWidths returns the stored widths",
        call(&adapter, "columnWidths", ""),
        Some("{\"size\":70}".into()),
    );
    c.eq(
        974,
        "columnNames child is the child's drawn rows",
        call(&adapter, "columnNames", "child"),
        Some("f1|f2".into()),
    );
    c.eq(
        974,
        "columnNames parent is the parent's drawn rows",
        call(&adapter, "columnNames", "parent"),
        Some("work|other".into()),
    );
    c.eq(
        975,
        "columnPeekNames reads a folder's last rows",
        call(&adapter, "columnPeekNames", "/srv"),
        Some("work|other".into()),
    );
    c.eq(
        628,
        "columnActiveRect is the middle track's rect",
        call(&adapter, "columnActiveRect", ""),
        Some("592 27 400 600".into()),
    );
    c.eq(
        629,
        "columnParentRowCentre centres a parent row",
        call(&adapter, "columnParentRowCentre", "1"),
        Some("392 74".into()),
    );
    c.eq(
        626,
        "columnChildRowCentre centres a child row",
        call(&adapter, "columnChildRowCentre", "0"),
        Some("1192 43".into()),
    );
    c.eq(
        627,
        "columnChildRowName names a child row",
        call(&adapter, "columnChildRowName", "1"),
        Some("f2".into()),
    );
    c.eq(
        627,
        "a child row past the end has no name",
        call(&adapter, "columnChildRowName", "9"),
        Some(String::new()),
    );
    c.eq(
        632,
        "a grandparent the count does not draw has no row",
        call(&adapter, "columnGrandparentRowCentre", "0"),
        Some(String::new()),
    );
    c.eq(
        633,
        "nor a great-grandparent",
        call(&adapter, "columnGreatGrandparentRowCentre", "0"),
        Some(String::new()),
    );
    c.eq(
        624,
        "columnChildEmpty reports the tile the states producer drew",
        call(&adapter, "columnChildEmpty", ""),
        Some("false 0.00 0.00".into()),
    );
    let state = call(&adapter, "columnsState", "").unwrap_or_default();
    c.ok(
        631,
        "the counters name the drawn tracks and the requests sent",
        state.contains("\"tracks\":3") && state.contains("\"peekRequests\":2"),
    );
    c.eq(
        601,
        "with no preview producer a preview read falls through",
        call(&adapter, "columnFrameReady", ""),
        None,
    );
    let producer = OneRead;
    adapter.preview = Some(&producer);
    c.eq(
        601,
        "a ported preview read is the producer's answer",
        call(&adapter, "columnFrameReady", ""),
        Some("true".into()),
    );
    c.eq(
        599,
        "an unported preview read still falls through",
        call(&adapter, "columnPlayerLoaded", ""),
        None,
    );
    c.finish();
}
