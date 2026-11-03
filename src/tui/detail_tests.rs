use super::*;
use crate::tui::{actions, input::Key, keymap::Map, render::{self, frame_tests}, theme::Theme, wire};
use std::time::Duration;

#[test]
fn i_cycles_all_modes_without_sorting_or_fetching_rows_in_every_flea_preset() {
    let map = Map::load();
    for preset in ["default", "vim", "windows", "mac", "yazi"] {
        let (mut wire, reader) = wire::echo_wire();
        let mut model = frame_tests::model(); model.preset = preset.into();
        let identity = (model.sort.clone(), model.cursor, model.top, model.current_path());
        for mode in [Column::Modified, Column::Mode, Column::Owner, Column::Size] {
            actions::key(&mut model, &Key::character('I', ""), &map, &mut wire).unwrap();
            assert_eq!(model.detail.mode, mode, "{preset}");
            assert_eq!(model.message, format!("Detail column · {}", mode.label()));
            assert!(model.message_at.elapsed() < Duration::from_secs(1));
            assert_eq!((model.sort.clone(), model.cursor, model.top, model.current_path()), identity);
            assert!(drain(&mut wire).is_empty());
        }
        wire::finish(wire, reader);
    }
}

#[test]
fn modified_cells_match_board_positions_for_folders_files_and_missing_dates() {
    let mut model = frame_tests::model();
    model.detail.mode = Column::Modified;
    for row in model.rows.values_mut() { row.modified = 1790158470; }
    let stamp = super::super::calendar::modified(1790158470);
    let output = render::frame(&model, &Theme::from_text(""), &Map::load(), 130, 31, Duration::ZERO);
    let grid = frame_tests::grid(&output, 130, 31);
    for line in grid.iter().skip(2).take(9) {
        assert_eq!(line.chars().skip(61).take(16).collect::<String>(), stamp);
        assert_eq!(line.chars().nth(79), Some('│'));
    }
    assert!(grid[3].contains(&stamp));
    assert!(grid[3].contains("99 B"), "preview size stays size when the detail changes");
    assert_eq!(value(&model, 0, &model.rows[&0]), stamp);
    model.rows.get_mut(&0).unwrap().modified = 0;
    assert!(value(&model, 0, &model.rows[&0]).is_empty());
    model.rows.get_mut(&0).unwrap().modified = i64::MAX;
    assert!(value(&model, 0, &model.rows[&0]).is_empty());
    model.detail.mode = Column::Mode;
    model.rows.get_mut(&0).unwrap().mode = 0o40755;
    assert_eq!(value(&model, 0, &model.rows[&0]), "0755");
    model.rows.get_mut(&0).unwrap().mode = 0o104755;
    assert_eq!(value(&model, 0, &model.rows[&0]), "4755");
    model.rows.get_mut(&0).unwrap().mode = 0;
    for mode in [Column::Size, Column::Modified, Column::Mode, Column::Owner] {
        model.detail.mode = mode;
        assert!(value(&model, 0, &model.rows[&0]).is_empty());
    }
}

fn answer(request: &Json, owner: &str) -> Json {
    Json::Obj(vec![("t".into(), word("owner")), ("row".into(), number(count(request, "row"))),
        ("token".into(), number(count(request, "token"))), ("owner".into(), word(owner))])
}

// Just past the backoff, so a sleep of this length always reopens a failed row.
const RETRY_WAIT: Duration = OWNER_RETRY_DELAY.saturating_add(Duration::from_millis(100));

fn retry_answer(request: &Json) -> Json {
    let mut reply = answer(request, "");
    if let Json::Obj(fields) = &mut reply { fields.push(("retry".into(), Json::Bool(true))); }
    reply
}

#[test]
fn failed_owners_retry_after_a_delay_without_starving_healthy_visible_rows() {
    let (mut wire, reader) = wire::echo_wire();
    let mut model = frame_tests::model();
    model.total = 100000;
    model.height = 3;
    model.detail.mode = Column::Owner;
    prepare(&mut model, &mut wire).unwrap();
    let sent = drain(&mut wire);
    let failed = requests(&sent, "owner")[0];
    model.receive(retry_answer(failed), &mut wire).unwrap();
    for (index, owner) in [(1, ""), (2, "healthy")] {
        prepare(&mut model, &mut wire).unwrap();
        let sent = drain(&mut wire);
        let request = requests(&sent, "owner")[0];
        assert_eq!(count(request, "row"), index, "a failed row must not block healthy rows");
        model.receive(answer(request, owner), &mut wire).unwrap();
    }
    prepare(&mut model, &mut wire).unwrap();
    assert!(drain(&mut wire).is_empty(), "a failed row backs off instead of spinning");
    std::thread::sleep(RETRY_WAIT);
    prepare(&mut model, &mut wire).unwrap();
    let sent = drain(&mut wire);
    let retried = requests(&sent, "owner");
    assert_eq!(retried.len(), 1, "a failed visible row retries without a new listing");
    assert_eq!(count(retried[0], "row"), 0);
    model.receive(answer(retried[0], "recovered"), &mut wire).unwrap();
    assert_eq!(value(&model, 0, &model.rows[&0]), "recovered");
    prepare(&mut model, &mut wire).unwrap();
    assert!(drain(&mut wire).is_empty(), "successful owners and terminal blanks stay cached");
    wire::finish(wire, reader);
}

#[test]
fn failed_owner_retries_stay_visible_and_respect_mode_and_row_identity() {
    let (mut wire, reader) = wire::echo_wire();
    let mut model = frame_tests::model();
    model.height = 1;
    model.detail.mode = Column::Owner;
    prepare(&mut model, &mut wire).unwrap();
    let sent = drain(&mut wire);
    let stale = retry_answer(requests(&sent, "owner")[0]);
    model.receive(stale.clone(), &mut wire).unwrap();
    std::thread::sleep(RETRY_WAIT);
    model.top = 1;
    model.rows.get_mut(&1).unwrap().mode = 0;
    prepare(&mut model, &mut wire).unwrap();
    assert!(drain(&mut wire).is_empty(), "an offscreen failed owner requests nothing");
    model.top = 0;
    model.detail.mode = Column::Size;
    prepare(&mut model, &mut wire).unwrap();
    assert!(drain(&mut wire).is_empty(), "another detail mode requests no owner retries");
    model.detail.mode = Column::Owner;
    model.pending = Some("/next".into());
    prepare(&mut model, &mut wire).unwrap();
    assert!(drain(&mut wire).is_empty(), "navigation still gates owner work");
    model.pending = None;
    model.invalidate_rows();
    model.rows.insert(0, frame_tests::entry("replacement", false, 1));
    prepare(&mut model, &mut wire).unwrap();
    let sent = drain(&mut wire);
    let fresh = requests(&sent, "owner")[0];
    model.receive(stale, &mut wire).unwrap();
    prepare(&mut model, &mut wire).unwrap();
    assert!(drain(&mut wire).is_empty(), "an old failure cannot spend the fresh pending request");
    model.receive(answer(fresh, "replacement-owner"), &mut wire).unwrap();
    assert_eq!(value(&model, 0, &model.rows[&0]), "replacement-owner");
    wire::finish(wire, reader);
}

#[test]
fn owners_are_serial_cached_and_bounded_to_visible_held_rows_at_100k() {
    let (mut wire, reader) = wire::echo_wire();
    let mut model = frame_tests::model(); model.total = 100000; model.height = 3;
    model.detail.mode = Column::Owner;
    for index in 9..100 { model.rows.insert(index, frame_tests::entry(&format!("row-{index}"), false, 1)); }
    let mut asked = Vec::new();
    for _ in 0..3 {
        prepare(&mut model, &mut wire).unwrap();
        prepare(&mut model, &mut wire).unwrap();
        let sent = drain(&mut wire); let owners = requests(&sent, "owner");
        assert_eq!(owners.len(), 1); let request = owners[0];
        asked.push(count(request, "row"));
        model.receive(answer(request, "gm"), &mut wire).unwrap();
        assert_eq!(value(&model, count(request, "row"), &model.rows[&count(request, "row")]), "gm");
    }
    assert_eq!(asked, [0, 1, 2]);
    prepare(&mut model, &mut wire).unwrap(); assert!(drain(&mut wire).is_empty());
    model.top = 97; model.cursor = 98;
    prepare(&mut model, &mut wire).unwrap();
    let sent = drain(&mut wire); let request = requests(&sent, "owner")[0];
    assert_eq!(count(request, "row"), 97);
    model.receive(answer(request, ""), &mut wire).unwrap();
    assert!(value(&model, 97, &model.rows[&97]).is_empty());
    model.top = 97; model.height = 1;
    prepare(&mut model, &mut wire).unwrap(); assert!(drain(&mut wire).is_empty(), "unknown owner is cached quietly");
    wire::finish(wire, reader);
}

#[test]
fn stale_owner_replies_cannot_attach_to_reused_rows_and_disabled_modes_fetch_nothing() {
    let (mut wire, reader) = wire::echo_wire();
    let mut model = frame_tests::model(); model.detail.mode = Column::Owner;
    prepare(&mut model, &mut wire).unwrap();
    let sent = drain(&mut wire); let request = requests(&sent, "owner")[0];
    let old = answer(request, "old-user");
    model.invalidate_rows(); model.rows.insert(0, frame_tests::entry("replacement", false, 1));
    prepare(&mut model, &mut wire).unwrap(); assert!(drain(&mut wire).is_empty(), "only one owner worker may be in flight");
    model.receive(old.clone(), &mut wire).unwrap();
    assert!(value(&model, 0, &model.rows[&0]).is_empty());
    prepare(&mut model, &mut wire).unwrap();
    let sent = drain(&mut wire); let fresh = requests(&sent, "owner")[0];
    model.receive(old, &mut wire).unwrap();
    assert!(value(&model, 0, &model.rows[&0]).is_empty());
    model.receive(answer(fresh, "new-user"), &mut wire).unwrap();
    assert_eq!(value(&model, 0, &model.rows[&0]), "new-user");
    for mode in [Column::Size, Column::Modified, Column::Mode] {
        model.detail.mode = mode; model.detail.invalidate();
        prepare(&mut model, &mut wire).unwrap(); assert!(drain(&mut wire).is_empty());
    }
    model.detail.mode = Column::Owner; model.pending = Some("/next".into());
    prepare(&mut model, &mut wire).unwrap(); assert!(drain(&mut wire).is_empty());
    wire::finish(wire, reader);
}

#[test]
fn detail_receipt_expires_at_four_seconds_without_changing_mode_or_error() {
    let mut model = frame_tests::model(); cycle(&mut model);
    let time = model.message_at;
    model.expire_message(time + Duration::from_millis(3999));
    assert_eq!(model.message, "Detail column · Modified");
    model.error = "Denied".into();
    model.expire_message(time + Duration::from_secs(4));
    assert!(model.message.is_empty()); assert_eq!(model.error, "Denied");
    assert_eq!(model.detail.mode, Column::Modified);
    model.hints.undo = "z".into(); model.say(crate::tui::prompts::undoable(&model, "Renamed".into()));
    model.expire_message(model.message_at + Duration::from_secs(7));
    assert_eq!(model.message, "Renamed · z undoes");
    model.expire_message(model.message_at + Duration::from_secs(8));
    assert!(model.message.is_empty(), "an undo receipt clears on its own after the error time");
}

#[test]
fn properties_omit_invalid_modified_and_accept_gio_deleted_wall_time() {
    let (mut wire, reader) = wire::echo_wire();
    let mut model = frame_tests::model(); model.sheet = true; model.properties = Some(Vec::new()); model.action_id = 8;
    for stamp in ["0", "9223372036854775807", "1790158470"] {
        let reply = crate::jsondoc::parse(&format!(r#"{{"t":"menuaction","op":"properties","id":8,"ok":true,"path":"/fx/v/work/notes.txt","kind":"Plain text document","bytes":99,"modified":{stamp},"deleted":"2025-06-14T09:30:00"}}"#)).unwrap();
        model.receive(reply, &mut wire).unwrap();
        let facts = model.properties.as_ref().unwrap();
        let modified: Vec<_> = facts.iter().filter(|(label, _)| label == "Modified").collect();
        assert_eq!(modified.len(), usize::from(stamp == "1790158470"));
        assert!(facts.contains(&("Deleted".into(), "2025-06-14 09:30".into())));
        assert!(facts.contains(&("Kind".into(), "Plain text document".into())));
    }
    wire::finish(wire, reader);
}

fn drain(wire: &mut Wire) -> Vec<Json> {
    wire.send(vec![("c", word("fence"))]).unwrap();
    let mut out = Vec::new();
    loop {
        let value = wire.events.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
        if text(&value, "c") == "fence" { return out; }
        out.push(value);
    }
}
fn requests<'a>(sent: &'a [Json], command: &str) -> Vec<&'a Json> {
    sent.iter().filter(|value| text(value, "c") == command).collect()
}

#[test]
fn calendar_frames_stay_identical_with_100k_logical_rows() {
    let mut model = frame_tests::model(); model.total = 100000; model.detail.mode = Column::Modified;
    for row in model.rows.values_mut() { row.modified = 1790158470; }
    let theme = Theme::from_text(""); let map = Map::load();
    let reference = render::frame(&model, &theme, &map, 130, 31, Duration::ZERO);
    let mut micros = Vec::new();
    for _ in 0..5 {
        let start = std::time::Instant::now();
        for _ in 0..100 {
            assert_eq!(render::frame(&model, &theme, &map, 130, 31, Duration::ZERO), reference);
        }
        micros.push(start.elapsed().as_micros() / 100);
    }
    eprintln!("t4 Modified frame: {}..{} microseconds/frame, five batches of 100, debug, nine held rows, 100000 logical rows", micros.iter().min().unwrap(), micros.iter().max().unwrap());
}

#[test]
fn preview_metadata_omits_the_whole_modified_fact_when_the_date_is_unprintable() {
    let mut model = frame_tests::model(); model.cursor = 0;
    for timestamp in [-1, 0, i64::MAX, 1790158470] {
        model.rows.get_mut(&0).unwrap().modified = timestamp;
        model.preview_path.clear();
        crate::tui::preview::load(&mut model, false);
        let modified: Vec<_> = model.preview_metadata.iter().filter(|line| line.starts_with("Modified · ")).collect();
        assert_eq!(modified.len(), usize::from(timestamp == 1790158470));
        if timestamp == 1790158470 { assert_eq!(modified[0], &format!("Modified · {}", crate::tui::calendar::modified(timestamp))); }
    }
}

#[test]
fn owner_metadata_cannot_emit_terminal_commands() {
    let mut model = frame_tests::model(); model.detail.mode = Column::Owner;
    model.detail.owners.insert(0, "gm\x1b]52;c;payload\x07\u{202e}".into());
    let frame = render::frame(&model, &Theme::from_text(""), &Map::load(), 130, 31, Duration::ZERO);
    assert!(!frame.contains("\x1b]52") && !frame.contains('\x07') && !frame.contains('\u{202e}'));
    assert!(frame.contains("gm]52;c;payload"));
}

#[test]
fn a_link_rests_without_a_size_as_the_board_and_v0312_draw_it() {
    let mut model = frame_tests::model();
    let row = model.rows.get_mut(&8).unwrap();
    row.link = "photo.png".into(); row.size = 9; row.mode = 0o120777;
    let name = row.name.clone();
    assert!(value(&model, 8, &model.rows[&8]).is_empty());
    let output = render::frame(&model, &Theme::from_text(""), &Map::load(), 130, 31, Duration::ZERO);
    let line = frame_tests::grid(&output, 130, 31).into_iter().find(|line| line.contains(&name)).unwrap();
    assert!(line.contains("→ photo.png") && !line.contains("9 B"), "{line}");
    model.detail.mode = Column::Modified;
    model.rows.get_mut(&8).unwrap().modified = 1790158470;
    assert_eq!(value(&model, 8, &model.rows[&8]), super::super::calendar::modified(1790158470));
}
