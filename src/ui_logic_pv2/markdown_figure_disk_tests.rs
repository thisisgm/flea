// Ported from tests/js/figureservicedisk.js: the persistent cache answers first, a miss reaches the helper whole, and an idle store is stopped.
use super::*;
use crate::ui_logic_pv2::markdown::markdown_figure_store::StoreEvent;

fn on_disk() -> Fake {
    let mut fake = Fake::new();
    fake.service.disk.available = true;
    fake
}

fn store_answers(fake: &mut Fake, id: u64, svg: &str) {
    let event = StoreEvent::Answered {
        id,
        svg: svg.into(),
    };
    fake.service.store_event(event, fake.now);
    fake.take();
}

#[test]
fn the_disk_answers_first_and_a_miss_reaches_the_helper() {
    let mut fake = on_disk();
    let remembered = fake.ask("remembered", true);
    assert!(
        fake.service.disk.owed() == 1 && fake.service.pending.is_empty() && !fake.service.starting
    );
    store_answers(&mut fake, remembered, "disk svg");
    assert!(fake.last_svg() == "disk svg" && fake.writes.is_empty() && !fake.service.starting);
    let unseen = fake.ask("unseen", true);
    store_answers(&mut fake, unseen, "");
    assert!(fake.service.starting, "a disk miss starts the helper");
    fake.start();
    fake.reply(unseen, "drawn svg");
    assert_eq!(
        fake.service.disk.puts, 1,
        "the helper's answer goes to the disk cache"
    );
    fake.service.store_started(fake.now);
    fake.take();
    let put = fake
        .store_lines
        .iter()
        .find(|line| line.contains("\"op\":\"put\""));
    assert!(put.is_some_and(|line| line.contains("drawn svg")));
}

#[test]
fn a_warm_query_names_each_figure_by_its_full_key() {
    let mut fake = on_disk();
    let theme_a = theme();
    let theme_b = Theme {
        accent: "#f7768e".into(),
        ..theme()
    };
    let figures = [
        (FigureKind::Math, "x^2".to_string()),
        (FigureKind::Mermaid, "flowchart TD\n    A --> B".to_string()),
    ];
    let themes = |kind| {
        Some(if kind == FigureKind::Math {
            theme_a.clone()
        } else {
            theme_b.clone()
        })
    };
    let warmed = fake.service.warm(&figures, &themes, fake.now);
    fake.take();
    assert!(warmed && fake.service.disk.owed() == 1 && !fake.service.starting);
    let math = cache_key(FigureKind::Math, "x^2", &theme_a, true);
    let mermaid = cache_key(FigureKind::Mermaid, &figures[1].1, &theme_b, true);
    assert_eq!(fake.service.warm_keys, vec![math.clone(), mermaid]);
    assert_ne!(
        math,
        cache_key(FigureKind::Math, "x^2", &theme_b, true),
        "another accent is another key"
    );
    let query = fake.service.warm_query;
    fake.service.store_event(
        StoreEvent::Known {
            id: query,
            all: false,
        },
        fake.now,
    );
    fake.take();
    assert!(
        fake.service.starting,
        "a query the store does not know starts the helper warm"
    );
}

#[test]
fn a_refusal_leaves_a_ticket_with_the_disk_to_the_disk_s_answer() {
    let mut fake = on_disk();
    let held = fake.ask("still with the disk", true);
    fake.exit(REFUSAL_EXIT);
    assert!(
        fake.answers.is_empty(),
        "a refusal does not fail a ticket still with the disk"
    );
    store_answers(&mut fake, held, "disk svg");
    assert_eq!(
        fake.answers,
        vec![(held, "disk svg".to_string(), String::new())]
    );
    let mut fake = on_disk();
    let missed = fake.ask("the disk misses it", true);
    let other = fake.ask("not on disk either", true);
    fake.exit(REFUSAL_EXIT);
    store_answers(&mut fake, missed, "");
    assert_eq!(
        fake.answers,
        vec![(missed, String::new(), NOT_STARTED.to_string())]
    );
    store_answers(&mut fake, other, "");
    assert!(
        fake.answers.len() == 2 && fake.answers[1].0 == other && fake.service.waiting.is_empty()
    );
}

#[test]
fn an_idle_store_is_stopped_and_asked_again_until_it_ends() {
    let mut fake = on_disk();
    // The store's reply and drain bounds are far off, so only the idle stop acts here.
    fake.service.disk.reply_ms = 1_000_000;
    fake.service.disk.drain_ms = 1_000_000;
    let figures = [(FigureKind::Math, "x^2".to_string())];
    fake.service.warm(&figures, &|_| Some(theme()), fake.now);
    fake.service.store_started(fake.now);
    fake.take();
    fake.tick();
    let idle = fake
        .service
        .idle_at
        .expect("a running store arms the idle stop");
    let closes = |fake: &mut Fake| {
        fake.service
            .take_commands()
            .into_iter()
            .filter(|command| *command == Command::StoreCloseInput)
            .count()
    };
    fake.now = idle;
    fake.service.tick(fake.now);
    assert_eq!(closes(&mut fake), 0, "a stop owed a reply is refused");
    let again = fake
        .service
        .idle_at
        .expect("a refused idle stop re-arms the idle timer");
    let line = format!("{{\"id\":{},\"known\":true}}", fake.service.warm_query);
    fake.service.store_line(&line, fake.now);
    fake.now = again;
    fake.service.tick(fake.now);
    assert_eq!(
        closes(&mut fake),
        1,
        "the next idle, with the reply landed, stops the store"
    );
    assert!(
        fake.service.idle_at.is_some(),
        "a draining store is asked again at the next idle"
    );
    let mut fake = Fake::new();
    fake.tick();
    assert!(
        fake.service.idle_at.is_none(),
        "a store that is not running is not asked again"
    );
}
