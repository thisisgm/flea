// Ported from tests/js/figureservice.js: the service's own handlers on a fake helper and a fake clock.
use super::*;

#[path = "markdown_figure_cold_tests.rs"]
mod cold;
#[path = "markdown_figure_disk_tests.rs"]
mod disk;
#[path = "markdown_figure_exit_tests.rs"]
mod exit;

/// The deadline the suites ran under.
pub const DEADLINE_MS: u64 = 1000;
const STAGGER_MS: u64 = 100;

pub fn theme() -> Theme {
    Theme {
        bg: "#101315".into(),
        fg: "#c0caf5".into(),
        accent: "#7aa2f7".into(),
        font: "monospace".into(),
        body_px: 14,
        ..Theme::default()
    }
}

// The service with what it wrote, killed and answered, in order; the persistent cache stands off unless a case turns it on.
pub struct Fake {
    pub service: Service,
    pub now: u64,
    pub writes: Vec<u64>,
    pub kills: usize,
    pub spawns: usize,
    pub store_lines: Vec<String>,
    pub answers: Vec<(u64, String, String)>,
}

impl Fake {
    pub fn new() -> Self {
        let mut service = Service {
            render_ms: DEADLINE_MS,
            ..Service::default()
        };
        service.disk.available = false;
        Fake {
            service,
            now: 0,
            writes: Vec::new(),
            kills: 0,
            spawns: 0,
            store_lines: Vec::new(),
            answers: Vec::new(),
        }
    }

    pub fn take(&mut self) {
        for command in self.service.take_commands() {
            match command {
                Command::HelperWrite(line) => {
                    let id = crate::jsondoc::parse(line.trim_end())
                        .ok()
                        .and_then(|message| message.get("id").and_then(|id| id.as_f64()));
                    self.writes.push(id.unwrap_or(-1.0) as u64);
                }
                Command::HelperKill => self.kills += 1,
                Command::HelperSpawn(_) => self.spawns += 1,
                Command::StoreWrite(line) => self.store_lines.push(line),
                Command::Done { ticket, svg, error } => self.answers.push((ticket, svg, error)),
                _ => {}
            }
        }
    }

    pub fn ask(&mut self, source: &str, display: bool) -> u64 {
        let id = self
            .service
            .ask(FigureKind::Math, source, display, &theme(), self.now);
        self.take();
        id
    }

    pub fn start(&mut self) {
        self.service.helper_started(self.now);
        self.take();
    }

    pub fn exit(&mut self, code: i32) {
        self.service.helper_exited(code, self.now);
        self.take();
    }

    pub fn tick(&mut self) {
        self.service.tick(self.now);
        self.take();
    }

    pub fn reply(&mut self, id: u64, svg: &str) {
        let line = format!("{{\"id\":{id},\"svg\":\"{svg}\"}}");
        self.service.helper_line(&line, self.now);
        self.take();
    }

    pub fn written(&self, id: u64) -> usize {
        self.writes.iter().filter(|written| **written == id).count()
    }

    pub fn drawn(&self) -> usize {
        self.answers
            .iter()
            .filter(|(_, svg, _)| !svg.is_empty())
            .count()
    }

    pub fn last_svg(&self) -> &str {
        self.answers.last().map_or("", |(_, svg, _)| svg.as_str())
    }
}

#[test]
fn an_answer_missing_from_written_keeps_the_last_written_deadline() {
    let mut fake = Fake::new();
    let absent = fake.ask("answered outside written", true);
    fake.start();
    fake.service.written.clear();
    let last = fake.ask("last written still waiting", true);
    fake.reply(absent, "absent svg");
    assert_eq!(fake.service.written.front(), Some(&last));
    assert_eq!(
        fake.answers[0].0, absent,
        "the absent ticket still answers its own request"
    );
    fake.now = DEADLINE_MS + 1;
    fake.tick();
    assert!(fake
        .answers
        .iter()
        .any(|(id, _, error)| *id == last && error == "render timed out"));
}

#[test]
fn inline_and_display_are_held_apart_and_a_revisit_writes_nothing() {
    let mut fake = Fake::new();
    let inline = fake.ask("\\sum_{n=1}^3 n", false);
    fake.start();
    fake.reply(inline, "inline svg");
    let display = fake.ask("\\sum_{n=1}^3 n", true);
    assert_eq!(
        fake.writes.len(),
        2,
        "display of the same source reaches the helper separately"
    );
    fake.reply(display, "display svg");
    fake.ask("\\sum_{n=1}^3 n", false);
    assert_eq!(fake.last_svg(), "inline svg");
    fake.ask("\\sum_{n=1}^3 n", true);
    assert_eq!(fake.last_svg(), "display svg");
    assert_eq!(fake.writes.len(), 2, "both cache hits write no helper line");
}

#[test]
fn a_figure_hit_before_each_store_outlives_cache_max_others() {
    let mut fake = Fake::new();
    let key = |source: &str| cache_key(FigureKind::Math, source, &theme(), true);
    fake.service.store(key("hot A"), "A svg".into());
    for i in 0..CACHE_MAX {
        fake.ask("hot A", true);
        fake.service
            .store(key(&format!("cold {i}")), "cold svg".into());
    }
    let before = fake.writes.len();
    let revisit = fake.ask("hot A", true);
    fake.start();
    assert!(fake
        .answers
        .iter()
        .any(|(id, svg, _)| *id == revisit && svg == "A svg"));
    assert_eq!(
        fake.writes.len(),
        before,
        "the revisit writes no helper line"
    );
}

#[test]
fn an_overdue_head_fails_alone_and_the_rest_retry_once() {
    let mut fake = Fake::new();
    let a = fake.ask("A", true);
    fake.start();
    fake.now = STAGGER_MS;
    let b = fake.ask("B", true);
    fake.now = DEADLINE_MS + 1;
    fake.tick();
    assert_eq!(fake.answers.len(), 1, "A's deadline fails only A");
    assert!(
        fake.service.waiting.contains_key(&b),
        "B waits for its resend"
    );
    assert_eq!(fake.answers[0].2, "render timed out");
    fake.tick();
    assert_eq!(
        fake.kills, 1,
        "repeated deadline ticks kill the helper once"
    );
    fake.reply(a, "late A");
    let late = fake.service.cached(FigureKind::Math, "A", &theme(), true);
    assert!(
        late.is_none(),
        "a killed generation cannot cache a late answer"
    );
    fake.exit(9);
    fake.start();
    assert_eq!(fake.written(b), 2, "B is resent once after A's kill");
    assert_eq!(fake.written(a), 1, "poison A is never resent");
    fake.now = DEADLINE_MS + STAGGER_MS + 1;
    fake.tick();
    assert_eq!(fake.kills, 1, "B gets a fresh turn after its resend");
    fake.reply(b, "B svg");
    assert_eq!(fake.last_svg(), "B svg");
}

#[test]
fn a_head_due_at_exactly_now_expires_so_the_timer_does_not_spin() {
    // figures.rs re-arms its timer for the next deadline; a head still waiting at that instant would re-arm 0 ms.
    let mut fake = Fake::new();
    let a = fake.ask("A", true);
    fake.start();
    let due = fake.service.waiting[&a].deadline;
    fake.now = due;
    fake.tick();
    assert_eq!(
        fake.answers.len(),
        1,
        "a deadline reached exactly fails its head"
    );
    assert_eq!(fake.answers[0].2, "render timed out");
}

#[test]
fn only_the_head_spends_a_deadline_and_each_queued_figure_gets_its_turn() {
    let mut fake = Fake::new();
    fake.ask("slow figure", true);
    fake.now = DEADLINE_MS + 1;
    fake.tick();
    assert!(
        fake.answers.is_empty(),
        "waiting for the helper to start spends no deadline"
    );
    fake.start();
    let formulas: Vec<u64> = (0..10)
        .map(|i| fake.ask(&format!("formula {i}"), true))
        .collect();
    assert!(formulas
        .iter()
        .all(|id| fake.service.waiting[id].deadline == 0));
    fake.now += DEADLINE_MS + 1;
    fake.tick();
    assert_eq!(fake.answers.len(), 1, "one slow figure fails only itself");
    fake.exit(9);
    fake.start();
    for (i, id) in formulas.iter().enumerate() {
        fake.now += DEADLINE_MS - STAGGER_MS;
        fake.tick();
        fake.reply(*id, &format!("formula svg {i}"));
    }
    assert_eq!(
        fake.drawn(),
        formulas.len(),
        "all ten queued formulas answer"
    );
    assert!(formulas.iter().all(|id| fake.written(*id) == 2));
    assert_eq!(fake.kills, 1, "ten queued turns cause no extra timeout");
}

#[test]
fn two_slow_figures_fail_themselves_and_a_healthy_one_survives_both() {
    let mut fake = Fake::new();
    let first = fake.ask("first slow", true);
    fake.start();
    let second = fake.ask("second slow", true);
    let healthy = fake.ask("healthy", true);
    fake.now += DEADLINE_MS + 1;
    fake.tick();
    fake.exit(9);
    fake.start();
    fake.now += DEADLINE_MS + 1;
    fake.tick();
    assert_eq!(fake.answers.len(), 2);
    assert_eq!(
        fake.answers[1].0, second,
        "the second timeout names its own ticket"
    );
    fake.exit(9);
    fake.start();
    fake.reply(healthy, "healthy svg");
    assert_eq!(fake.last_svg(), "healthy svg");
    assert_eq!(fake.written(healthy), 3, "resent once for each kill");
    assert_eq!(fake.written(first), 1);
    assert_eq!(fake.written(second), 2);
}

#[test]
fn an_ask_while_the_helper_stops_waits_for_the_next_helper() {
    let mut fake = Fake::new();
    fake.ask("old helper", true);
    fake.start();
    fake.now = DEADLINE_MS + 1;
    fake.tick();
    let next = fake.ask("during stopping", true);
    assert!(fake.service.stopping, "a kill leaves the helper stopping");
    assert_eq!(
        fake.writes.len(),
        1,
        "nothing is written to the dying process"
    );
    fake.exit(9);
    assert!(
        fake.service.starting,
        "the exit starts a helper for the queued ask"
    );
    fake.start();
    assert_eq!(
        fake.writes,
        vec![1, next],
        "the fresh helper gets the queued ticket once"
    );
    fake.reply(next, "fresh svg");
    assert_eq!(fake.last_svg(), "fresh svg");
}
