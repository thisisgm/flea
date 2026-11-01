use super::*;

fn i18n() -> crate::i18n::I18n {
    crate::i18n::test_catalogs::english()
}

fn resting() -> Player {
    Player::resting(Path::new("/nonexistent/clip.mp4"), Protocol::Kitty, true, (40, 30, 81, 3), (160, 50), (800, 600))
}

#[test]
fn a_resting_player_waits_for_the_probe_then_draws_the_controls_mpv_would() {
    let mut player = resting();
    assert!(player.at_rest());
    assert_eq!(player.line(60, true, &i18n()), "Loading media…");
    player.probe(10_000);
    let line = player.line(60, true, &i18n());
    assert!(line.starts_with("[Play ]  ●"), "{line}");
    assert!(line.ends_with("0:00 / 0:10"), "{line}");
    player.probe(99_000);
    assert!(player.line(60, true, &i18n()).ends_with("0:00 / 0:10"), "a second probe never moves the length");
}

#[test]
fn a_seek_asked_at_rest_stays_inside_the_clip() {
    assert_eq!(rest_target(0.0, 10.0, -5), 0.0);
    assert_eq!(rest_target(8.0, 10.0, 5), 10.0);
    assert_eq!(rest_target(2.0, 0.0, 5), 7.0);
}

#[test]
fn play_at_rest_that_cannot_start_mpv_leaves_the_controls_as_they_were() {
    let mut player = resting();
    player.probe(10_000);
    assert!(player.toggle().is_err(), "the test file does not exist, so mpv never starts");
    assert!(player.at_rest() && player.paused && player.resume.is_none());
    assert!(player.line(60, true, &i18n()).starts_with("[Play ]"), "{}", player.line(60, true, &i18n()));
    assert!(player.seek(5).is_err() && player.position == 0.0);
}

#[test]
fn a_seek_or_toggle_before_mpv_connects_waits_in_resume() {
    let mut player = resting();
    player.probe(10_000);
    player.force_starting_for_test();
    player.toggle().unwrap();
    assert!(!player.paused && player.resume == Some((0.0, false)));
    player.seek(5).unwrap();
    assert_eq!(player.resume, Some((5.0, false)));
    player.toggle().unwrap();
    assert_eq!(player.resume, Some((5.0, true)));
    assert!(player.line(60, true, &i18n()).starts_with("[Play ]"), "{}", player.line(60, true, &i18n()));
}

#[test]
fn only_the_file_that_was_playing_resumes_at_its_place() {
    let playback = (PathBuf::from("/m/a.mp4"), 42.0, true);
    assert_eq!(resume_for(Some(&playback), Path::new("/m/a.mp4")), Some((42.0, true)));
    assert_eq!(resume_for(Some(&playback), Path::new("/m/b.mp4")), None);
    assert_eq!(resume_for(None, Path::new("/m/a.mp4")), None);
}

// At rest the controls keep their cells and lose their brackets and diamond, so nothing moves when focus enters.
#[test]
fn brackets_and_the_diamond_draw_only_while_the_preview_holds_the_keyboard() {
    let mut player = resting();
    player.probe(10_000);
    let rest = player.line(60, false, &i18n());
    assert!(rest.starts_with(" Play    ●"), "{rest}");
    assert!(!rest.contains('[') && !rest.contains('◆'), "{rest}");
    assert_eq!(crate::tui::cells::text_width(&rest), crate::tui::cells::text_width(&player.line(60, true, &i18n())));
    player.control = 1;
    let seek = player.line(60, true, &i18n());
    assert!(seek.starts_with(" Play    ◆") && !seek.contains('['), "{seek}");
    let runs = player.runs(60, true, &i18n());
    assert_eq!(runs.iter().map(|run| run.0).collect::<Vec<_>>(), [Ink::Foreground, Ink::Frame, Ink::Foreground, Ink::Frame, Ink::Muted]);
}

// These are real poll/connect branches with owned children and a private in-memory IPC stream.
#[test]
fn tui_preview_media_failures_retain_semantic_words() {
    use crate::tui::preview::words::Text;
    let mut starting=resting(); starting.process=Some(Process::idle());
    starting.started=Instant::now()-Duration::from_secs(4);
    starting.poll();
    assert!(matches!(starting.failure,Some(Text::Key("tui.preview.media_start"))));
    assert!(!starting.error.is_empty());
    let mut unplayable=resting(); unplayable.process=Some(Process::idle());
    let (socket,_peer)=UnixStream::pair().unwrap(); unplayable.socket=Some(socket);
    let (sender,events)=mpsc::channel(); unplayable.events=Some(events);
    sender.send(jsondoc::parse(r#"{"event":"end-file","reason":"error"}"#).unwrap()).unwrap();
    unplayable.poll();
    assert!(matches!(unplayable.failure,Some(Text::Key("media.unplayable"))));
    assert!(!unplayable.error.is_empty());
    let mut stopped=resting(); stopped.process=Some(Process::finished());
    let (socket,_peer)=UnixStream::pair().unwrap(); stopped.socket=Some(socket);
    let (_sender,events)=mpsc::channel(); stopped.events=Some(events);
    stopped.poll();
    assert!(matches!(stopped.failure,Some(Text::Key("tui.preview.media_stopped"))));
    assert!(!stopped.error.is_empty());
}
