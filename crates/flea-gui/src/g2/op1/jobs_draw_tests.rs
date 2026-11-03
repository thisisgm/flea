//! Jobs card geometry measured from the shared overlay fixture.
use super::{measured, open_card, TestAppContext};

// The Jobs card sits 14 from the right and 36 from the bottom at its board width.
#[gpui::test]
fn the_jobs_card_sits_at_its_board_place(cx: &mut TestAppContext) {
    use flea::ui_logic_op1::card;
    let (_, vcx) = open_card(cx, 1100., 700., |o| {
        let cat = o.catalog.clone();
        o.model.line(&cat, r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copying 10 of 18 to NAS","file":"DSC_4944.jpg","done":9,"total":18,"bytes":5300000,"totalBytes":9700000,"itemBytes":458300,"rate":1100000,"left":4,"state":"running","reason":"","output":[],"undo":false,"cancelled":false},"unfinished":1}"#, 0);
    });
    let jobs = measured(vcx, "op1-jobs");
    assert_eq!(f32::from(jobs.size.width), card::JOBS_WIDTH);
    assert_eq!(
        f32::from(jobs.origin.x + jobs.size.width),
        1100. - card::JOBS_RIGHT
    );
    assert_eq!(
        f32::from(jobs.origin.y + jobs.size.height),
        700. - card::JOBS_BOTTOM
    );
    assert!(vcx.debug_bounds("op1-scrim").is_none());
}

// jobs.html: 16 px marks, a 6 px bar across the track, and full-width hairlines 12 from each block.
#[gpui::test]
fn the_jobs_card_draws_its_blocks_at_board_sizes(cx: &mut TestAppContext) {
    use flea::ui_logic_op1::card;
    let (_, vcx) = open_card(cx, 1100., 700., |o| {
        let cat = o.catalog.clone();
        o.model.line(&cat, r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copying 10 of 18 to NAS","file":"DSC_4944.jpg","done":9,"total":18,"bytes":5300000,"totalBytes":9700000,"itemBytes":458300,"rate":1100000,"left":4,"state":"running","reason":"","output":[],"undo":false,"cancelled":false},"unfinished":1}"#, 0);
        o.model.line(&cat, r#"{"t":"job","job":{"id":14,"kind":"send","head":"Sending 2 of 3 to gdrive","file":"","done":0,"total":3,"bytes":0,"totalBytes":0,"itemBytes":0,"rate":0,"left":null,"state":"failed","reason":"DSC_4930.jpg did not match on b2-photos.","output":[],"undo":false,"cancelled":false},"unfinished":1}"#, 0);
    });
    let card_bounds = measured(vcx, "op1-jobs");
    let mark = measured(vcx, "op1-job-12-mark");
    let bar = measured(vcx, "op1-job-12-bar");
    let fill = measured(vcx, "op1-bar-fill");
    let rule = measured(vcx, "op1-jobs-rule-1");
    let failed = measured(vcx, "op1-job-14");
    let running = measured(vcx, "op1-job-12");
    assert_eq!(
        (f32::from(mark.size.width), f32::from(mark.size.height)),
        (card::JOBS_MARK, card::JOBS_MARK)
    );
    assert_eq!(f32::from(bar.size.height), card::JOBS_BAR);
    assert_eq!(f32::from(bar.size.width), card::jobs_track(1100.));
    // The exact share of the track, as the board's percentage draws it, within the pixel the layout snaps to.
    let share = card::jobs_track(1100.) * 5_300_000. / 9_700_000.;
    assert!(
        (f32::from(fill.size.width) - share).abs() < 1.,
        "fill {} for {share}",
        f32::from(fill.size.width)
    );
    assert_eq!(
        f32::from(rule.size.width),
        card::JOBS_WIDTH - 2. * card::BORDER
    );
    assert_eq!(
        f32::from(rule.origin.x - card_bounds.origin.x),
        card::BORDER
    );
    // The failed job is newest, so it sits on top; each block keeps 9 and 3 from the hairline.
    assert!(failed.origin.y < running.origin.y);
    assert_eq!(
        f32::from(rule.origin.y - (failed.origin.y + failed.size.height)),
        card::JOBS_GAP + card::JOBS_RULE_MARGIN
    );
    assert_eq!(
        f32::from(running.origin.y - (rule.origin.y + rule.size.height)),
        card::JOBS_GAP + card::JOBS_RULE_MARGIN
    );
    // Negative: a failed job draws no bar.
    assert!(vcx.debug_bounds("op1-job-14-bar").is_none());
}
