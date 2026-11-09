// Ported from tests/js/figureserviceexit.js: unexpected exits through the production handlers, with no running helper and no clock wait.
use super::*;

const CPU_CAP_EXIT: i32 = 137;
const CRASH_EXIT: i32 = 42;

#[test]
fn an_unexpected_exit_strikes_only_the_head_and_resends_the_rest() {
    let mut fake = Fake::new();
    let first = fake.ask("innocent first", true);
    fake.start();
    let second = fake.ask("innocent second", true);
    fake.exit(CPU_CAP_EXIT);
    assert!(
        fake.answers.is_empty(),
        "one CPU-cap exit fails no innocent ticket"
    );
    fake.start();
    fake.reply(first, "first svg");
    fake.reply(second, "second svg");
    assert_eq!(fake.drawn(), 2);
    assert_eq!(fake.writes.len(), 4, "each innocent ticket is resent once");
    let mut fake = Fake::new();
    let poison = fake.ask("crashes the helper", true);
    fake.start();
    let neighbour = fake.ask("innocent neighbour", true);
    let tail = fake.ask("innocent tail", true);
    fake.exit(CRASH_EXIT);
    assert!(
        fake.answers.is_empty(),
        "the first head strike answers no ticket"
    );
    fake.start();
    fake.exit(CRASH_EXIT);
    assert_eq!(
        fake.answers.len(),
        1,
        "two head strikes fail only the crashing ticket"
    );
    assert!(fake.answers[0].0 == poison && fake.answers[0].2.contains("exited 42"));
    fake.start();
    fake.reply(neighbour, "neighbour svg");
    fake.reply(tail, "tail svg");
    assert_eq!(
        fake.drawn(),
        2,
        "both neighbours render after the poison fails"
    );
    assert_eq!(fake.written(poison), 2, "the failed poison never returns");
    assert!(
        fake.service.available,
        "an unexpected exit keeps the service available"
    );
}

#[test]
fn a_refusal_fails_every_ticket_and_latches() {
    let mut fake = Fake::new();
    fake.ask("refused first", true);
    fake.start();
    fake.ask("refused second", true);
    fake.exit(REFUSAL_EXIT);
    assert_eq!(fake.answers.len(), 2, "127 fails every written ticket");
    assert!(fake.service.waiting.is_empty() && !fake.service.available);
}

#[test]
fn the_service_s_own_kill_charges_no_neighbour_a_strike() {
    let mut fake = Fake::new();
    let overdue = fake.ask("overdue head", true);
    fake.start();
    let survivor = fake.ask("survivor of the kill", true);
    fake.now = DEADLINE_MS + 1;
    fake.tick();
    assert_eq!(fake.service.waiting[&survivor].strikes, 0);
    fake.exit(CPU_CAP_EXIT);
    fake.start();
    fake.exit(CRASH_EXIT);
    assert_eq!(
        fake.answers.len(),
        1,
        "a neighbour of a timed-out head survives its first crash"
    );
    assert!(fake.answers[0].0 == overdue && fake.answers[0].2 == "render timed out");
    fake.start();
    fake.reply(survivor, "survivor svg");
    assert_eq!(fake.last_svg(), "survivor svg");
    let mut fake = Fake::new();
    fake.ask("overdue while stopping", true);
    fake.start();
    let bystander = fake.ask("written beside the overdue head", true);
    assert_eq!(
        fake.service.written.iter().position(|id| *id == bystander),
        Some(1)
    );
    fake.now = DEADLINE_MS + 1;
    fake.tick();
    assert!(fake.service.stopping && fake.service.written.is_empty());
    let before = fake.writes.len();
    fake.ask("arrives while stopping", true);
    assert!(
        fake.writes.len() == before && fake.service.written.is_empty(),
        "held back"
    );
    fake.exit(CPU_CAP_EXIT);
    assert_eq!(fake.service.waiting[&bystander].strikes, 0);
    assert_eq!(
        fake.answers.len(),
        1,
        "the exit that follows a kill fails nothing more"
    );
}
