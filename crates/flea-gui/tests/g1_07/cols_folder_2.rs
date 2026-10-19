//! tests/columnsfolder.qml w25: a manual-hold wait handed to a folder, a burst into two pending folders, and the bounded fallback.
use super::check::Checks;
use super::cols_folder::{
    phase_b, phase_c, phase_empty, phase_file, phase_null, Probe, SOURCE, STUDIO,
};
use super::fixture::row;
use flea_gui::navigation::history::join;

/// w25) a manual-hold class keeps c.png's held frame while the cursor waits on w1, and w1's landing releases it.
fn phase_manual(c: &mut Checks, probe: &mut Probe) {
    probe.cursor(2);
    probe.rows(vec![row("c.png", false), row("w1", true)]);
    probe.area.settings.manual_hold = true;
    probe.cursor(0);
    // Ctrl+Space: the preview's held load takes a picture on the idle swap.
    probe.p.swap.holding = true;
    probe.mark();
    probe.cursor(1);
    // The manual wait forms by the first 20 ms poll, and the probe answers w1 then.
    probe.run_to(3156);
    probe.answer(&[("kid.txt", false)]);
    probe.run_to(3196);
    let shown = probe.area.host.shown.clone();
    c.eq(
        322,
        "w25) a landed folder peek keeps the old column, want the folder",
        shown.is_dir,
        true,
    );
    c.eq(
        324,
        "w25) a landed folder peek shows <shownChildPath>, want <childPath()>",
        shown.child.clone(),
        probe.child_path(),
    );
    c.eq(
        327,
        "w25) a landed folder draws no peeked rows, want the answered folder",
        probe.rows_for(&shown.child),
        vec!["kid.txt".to_owned()],
    );
    c.eq(
        329,
        "w25) a landed folder keeps the preview visible, want it hidden",
        probe.preview_visible(),
        false,
    );
    c.ok(
        332,
        "w25) a landed folder peek leaves the third swap holding, want idle",
        probe.idle(),
    );
}

/// w25) d.png's held work hands over to two pending folder moves; only the last move's own fallback ends the wait.
fn phase_pending(c: &mut Checks, probe: &mut Probe) {
    probe.area.settings.manual_hold = false;
    probe.cursor(0);
    probe.rows(vec![
        row("c.png", false),
        row("d.png", false),
        row("w2", true),
        row("w3", true),
    ]);
    probe.cursor(1);
    // The new key ends the hold by burst, the queued show draws d.png, and the preview's load takes a fresh hold.
    probe.release();
    probe.captured();
    probe.p.swap.holding = true;
    probe.run_to(3202);
    probe.mark();
    probe.cursor(2);
    probe.run_to(3272);
    c.ok(
        355,
        "w25) the first pending folder move leaves the third swap idle, want the live picture kept",
        probe.held(),
    );
    probe.cursor(3);
    // A fallback started rather than restarted at the w2 move would come due here.
    probe.run_to(3356);
    probe.tick();
    probe.run_to(3382);
    c.ok(
        362,
        "w25) the old work cap over two pending moves leaves the third swap idle, want the live picture kept",
        probe.held(),
    );
    c.eq(
        364,
        "w25) a doubly waiting folder shows early, want the old column kept",
        probe.area.host.shown.is_dir,
        false,
    );
    probe.run_to(3622);
    let shown = probe.area.host.shown.clone();
    let child = probe.child_path();
    c.eq(
        372,
        "w25) an unanswered double wait keeps the old column, want the pending folder",
        shown.is_dir,
        true,
    );
    c.eq(
        374,
        "w25) the fallback shows <shownChildPath>, want <childPath()>",
        shown.child,
        child.clone(),
    );
    c.eq(
        376,
        "w25) the fallback wait answers without a peek, want it pending",
        probe.answered(&child),
        false,
    );
    c.eq(
        378,
        "w25) the fallback keeps the preview visible, want it hidden",
        probe.preview_visible(),
        false,
    );
    c.ok(
        381,
        "w25) an unanswered double wait leaves the third swap holding, want idle",
        probe.idle(),
    );
}

#[test]
fn a_folder_cursor_waits_by_data_and_keeps_the_live_picture() {
    let mut c = Checks::new(SOURCE);
    let mut probe = Probe::start();
    phase_null(&mut c, &mut probe);
    phase_empty(&mut c, &mut probe);
    phase_file(&mut c, &mut probe);
    phase_b(&mut c, &mut probe);
    phase_c(&mut c, &mut probe);
    phase_manual(&mut c, &mut probe);
    phase_pending(&mut c, &mut probe);
    // The failure list, the two assert helpers, the phase timers, the early reports and the PASS line are this test's own steps.
    c.covers(&[
        104, 133, 139, 146, 148, 149, 150, 152, 153, 154, 155, 156, 157, 158, 159, 160, 162, 163,
        180, 203, 209, 217, 226, 241, 254, 272, 277, 285, 303, 333, 348, 365, 384, 389, 398,
    ]);
    c.finish();
}

/// ui/ColumnsArea.qml:358 and :378 turn the empty tile's entrance off for the landing turn alone.
#[test]
fn a_settled_entrance_lasts_one_landing() {
    let mut c = Checks::new(SOURCE);
    let mut probe = Probe::start();
    probe.run_to(800);
    probe.rows(vec![row("sub", true), row("a.txt", false)]);
    probe.run_to(1100);
    probe.answer(&[]);
    // The cursor leaves for the file and comes back onto the answered empty folder, which shows at once.
    probe.cursor(1);
    probe.captured();
    probe.release();
    probe.cursor(0);
    let sub = join(STUDIO, "sub");
    c.ok(
        201,
        "d) a data-held empty landing leaves its entrance off",
        probe.area.hero_settled.as_deref() != Some(sub.as_str()),
    );
    c.finish();
}
