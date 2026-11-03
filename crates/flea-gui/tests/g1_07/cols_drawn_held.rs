//! tests/columnsdrawn.qml and tests/columns-held.qml: the child slot names its own folder's rows, and a held key over files asks nothing.
use super::check::Checks;
use super::cols_folder::{Probe, STUDIO};
use super::fixture::{reply, row, Pane, Producers, SETTINGS};
use flea::protocol::RowFacts;
use flea_gui::columns::Area;
use flea_gui::ipc::{columns::ColumnsAdapter, Adapter};

/// drawnNames("child") through the IPC adapter, as ui/Ipc.qml reads it.
fn child_names(probe: &Probe) -> String {
    probe.pane.with(|view| {
        let adapter = ColumnsAdapter {
            area: &probe.area,
            lens: view.lens,
            origin: (0., 0.),
            height: 600.,
            widths: "{}",
            child_empty: None,
            preview: None,
        };
        adapter
            .call("columnNames", &["child".to_owned()])
            .map(|answer| answer.stdout.trim_end().to_owned())
            .unwrap_or_default()
    })
}

#[test]
fn the_child_slot_names_the_cursor_folders_own_rows() {
    let mut c = Checks::new("tests/columnsdrawn.qml");
    let mut probe = Probe::start();
    probe.run_to(600);
    probe.pane.cursor = 0;
    probe.rows(vec![row("p1.txt", false), row("p2.txt", false)]);
    // The file's hold captures a frame later and shows p1.txt; its meta never answers.
    probe.run_to(616);
    probe.captured();
    probe.release();
    probe.run_to(1000);
    c.eq(
        111,
        "a cursor on a file draws child names '<drawnNames(child)>', want none",
        child_names(&probe),
        String::new(),
    );
    probe.rows(vec![
        row("outer", true),
        row("p1.txt", false),
        row("p2.txt", false),
    ]);
    probe.answer(&[("in1.txt", false), ("in2.txt", false)]);
    probe.run_to(1400);
    c.eq(
        122,
        "a cursor on a folder draws child names '<drawnNames(child)>', want its peeked in1.txt|in2.txt",
        child_names(&probe),
        "in1.txt|in2.txt".to_owned(),
    );
    // The failure list, the phase timers and the report are this test's own steps.
    c.covers(&[95, 99, 107, 112, 125, 130]);
    c.finish();
}

/// The swap's hold calls and the preview's settle arms the host made so far.
fn held_calls(probe: &Probe) -> (usize, usize) {
    let holds = probe
        .p
        .swap
        .calls
        .iter()
        .filter(|call| call.starts_with("hold "))
        .count();
    (holds, probe.p.preview.count("armSettle"))
}

#[test]
fn a_held_key_over_files_asks_no_peek_and_a_rows_replacement_takes_no_hold() {
    let mut c = Checks::new("tests/columns-held.qml");
    // A 1280 by 800 columns pane over its held window of empty files, the cursor on the first.
    let rows: Vec<RowFacts> = (1..=512)
        .map(|i| row(&format!("file-{i:05}.txt"), false))
        .collect();
    let here = format!("{STUDIO}/held");
    let mut probe = Probe {
        area: Area::new(1280., 800., None, 0., SETTINGS),
        pane: Pane::at(&here, rows),
        p: Producers::default(),
        now: 0,
        mark: 0,
    };
    probe.sync();
    let parent = reply(STUDIO, &[("held", true)]);
    {
        let Probe {
            area, pane, p, now, ..
        } = &mut probe;
        let now = *now;
        pane.with(|view| area.peeked(&parent, &view, &mut p.ports(), now));
    }
    let mut sweeps = Vec::new();
    for sweep in 0..3 {
        // g after a rest, a second's rest, then 70 j presses 25 ms apart with no release.
        probe.run_to(probe.now + 1500);
        probe.cursor(0);
        probe.run_to(probe.now + 1000);
        let peeks = probe.area.peek_requests;
        let calls = held_calls(&probe);
        for press in 1..=70 {
            probe.run_to(probe.now + 25);
            probe.cursor(probe.pane.cursor + 1);
            // Sweep 1 re-requests the held window at press 35: the same rows land again under the cursor.
            if sweep == 1 && press == 35 {
                let same = probe.pane.rows.clone();
                probe.rows(same);
            }
        }
        probe.run_to(probe.now + 1000);
        c.eq(
            174,
            &format!(
                "sweep {sweep} columns asks the backend no more than the list plus its loads' own asks"
            ),
            probe.area.peek_requests - peeks,
            0,
        );
        let (holds, arms) = held_calls(&probe);
        sweeps.push((holds - calls.0, arms - calls.1));
    }
    c.eq(
        188,
        "rows replacement under the burst takes no hold and no load of its own",
        sweeps[1],
        sweeps[0],
    );
    // The failure line, the judge and finish calls, the step timer and the stall watchdog are this test's own steps.
    c.covers(&[49, 246, 247, 251, 271]);
    c.finish();
}
