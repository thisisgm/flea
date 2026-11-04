// Ported from tests/js/figurestore.js: the persistent cache's client on a fake process and a fake clock.
use super::*;

const KEY: &str = "math\n#101315|#c0caf5|||||||monospace|14|7|0|0\ntrue\nx^2";

// The client with what it asked of its process and what it answered, kept in order.
struct Fake {
    store: Store,
    now: u64,
    writes: Vec<String>,
    kills: usize,
    closes: usize,
    spawns: usize,
    answers: Vec<(u64, String)>,
    knowns: Vec<(u64, bool)>,
}

impl Fake {
    fn new() -> Self {
        Fake {
            store: Store::default(),
            now: 0,
            writes: Vec::new(),
            kills: 0,
            closes: 0,
            spawns: 0,
            answers: Vec::new(),
            knowns: Vec::new(),
        }
    }

    fn take(&mut self, events: Vec<StoreEvent>) {
        for event in events {
            match event {
                StoreEvent::Answered { id, svg } => self.answers.push((id, svg)),
                StoreEvent::Known { id, all } => self.knowns.push((id, all)),
            }
        }
        for command in self.store.take_commands() {
            match command {
                Command::StoreWrite(line) => self.writes.push(line),
                Command::StoreKill => self.kills += 1,
                Command::StoreCloseInput => self.closes += 1,
                Command::StoreSpawn => self.spawns += 1,
                _ => {}
            }
        }
    }

    fn get(&mut self, id: u64) {
        let events = self.store.get(id, KEY, self.now);
        self.take(events);
    }

    fn ask(&mut self, id: u64) {
        let events = self.store.ask(id, &[KEY.to_string()], self.now);
        self.take(events);
    }

    fn put(&mut self, svg: &str) {
        self.store.put(KEY, svg, self.now);
        self.take(Vec::new());
    }

    fn start(&mut self) {
        self.store.started(self.now);
        self.take(Vec::new());
    }

    fn exit(&mut self) {
        let events = self.store.exited(self.now);
        self.take(events);
    }

    fn tick(&mut self) {
        let events = self.store.tick(self.now);
        self.take(events);
    }

    fn stop(&mut self) -> bool {
        let stopped = self.store.stop(self.now);
        self.take(Vec::new());
        stopped
    }

    fn reply(&mut self, line: &str) {
        let events = self.store.receive(line);
        self.take(events);
    }

    fn op(&self, i: usize) -> &str {
        let line = &self.writes[i];
        ["get", "put", "known"]
            .into_iter()
            .find(|op| line.contains(&format!("\"op\":\"{op}\"")))
            .unwrap_or("")
    }
}

#[test]
fn an_owed_reply_refuses_a_stop_and_a_stop_drains_instead_of_killing() {
    let mut fake = Fake::new();
    fake.ask(7);
    fake.start();
    assert_eq!(fake.store.owed(), 1, "a known query is owed a reply");
    assert!(
        !fake.stop(),
        "an idle stop is refused while a reply is owed"
    );
    assert!(fake.kills == 0 && fake.closes == 0 && fake.store.running && !fake.store.stopping);
    fake.reply("{\"id\":7,\"known\":true}");
    assert_eq!(fake.knowns, vec![(7, true)], "the owed reply still lands");
    assert!(fake.store.owed() == 0 && fake.store.available);
    assert!(fake.stop(), "with nothing owed the stop is accepted");
    assert!(
        fake.closes == 1 && fake.store.running && fake.kills == 0,
        "it closes stdin and drains"
    );
    fake.exit();
    assert!(
        fake.store.available && fake.store.exits == 1 && !fake.store.stopping,
        "the exit at EOF after a stop is no failure"
    );
}

#[test]
fn a_line_sent_while_the_store_drains_waits_for_the_next_store() {
    let mut fake = Fake::new();
    fake.put("<svg/>");
    fake.start();
    assert!(fake.writes.len() == 1 && fake.op(0) == "put" && fake.store.puts == 1);
    assert!(
        fake.stop(),
        "a put has no reply, so a stop right after it is accepted"
    );
    assert!(fake.store.running && fake.closes == 1 && fake.kills == 0);
    fake.get(8);
    assert!(fake.writes.len() == 1 && fake.store.queued.len() == 1 && fake.store.owed() == 1);
    fake.exit();
    assert!(
        fake.store.starting && fake.spawns == 2,
        "the exit starts the store again for the queued line"
    );
    fake.start();
    assert!(fake.writes.len() == 2 && fake.op(1) == "get" && fake.store.available);
}

#[test]
fn a_late_reply_ends_the_store_and_falls_to_the_helper() {
    let mut fake = Fake::new();
    fake.get(9);
    fake.start();
    fake.now = REPLY_MS + 1;
    fake.tick();
    assert!(
        !fake.store.available && fake.answers == vec![(9, String::new())] && fake.store.owed() == 0
    );
    assert_eq!(fake.kills, 1, "the late store is killed");
    let mut fake = Fake::new();
    fake.get(10);
    fake.start();
    fake.now = REPLY_MS - 1;
    fake.tick();
    assert!(
        fake.store.available && fake.answers.is_empty(),
        "a reply inside replyMs is waited for"
    );
}

#[test]
fn a_queued_line_is_owed_only_once_it_is_written() {
    let mut fake = Fake::new();
    fake.put("<svg/>");
    fake.start();
    assert!(fake.stop());
    fake.get(12);
    fake.now = 1;
    fake.tick();
    assert!(fake.store.available && fake.answers.is_empty() && fake.kills == 0);
    fake.exit();
    fake.start();
    fake.tick();
    assert!(
        fake.store.available && fake.answers.is_empty() && fake.writes.len() == 2,
        "its clock starts when written"
    );
    fake.now += REPLY_MS + 1;
    fake.tick();
    assert!(
        !fake.store.available && fake.answers.len() == 1,
        "a reply that then outlasts replyMs ends the store"
    );
}

#[test]
fn a_slow_drain_keeps_its_store_and_only_its_put_waits() {
    let mut fake = Fake::new();
    fake.put("<svg/>");
    fake.start();
    assert!(fake.stop());
    fake.get(14);
    fake.ask(15);
    fake.put("<svg>2</svg>");
    fake.now = DRAIN_MS - 1;
    fake.tick();
    assert!(fake.answers.is_empty() && fake.knowns.is_empty() && fake.store.owed() == 2);
    fake.now = DRAIN_MS + 1;
    fake.tick();
    assert_eq!(
        fake.answers,
        vec![(14, String::new())],
        "past drainMs a queued get is a miss"
    );
    assert!(fake.knowns == vec![(15, false)] && fake.store.owed() == 0);
    assert!(fake.kills == 0 && fake.store.hang_kills == 0 && fake.store.available);
    assert!(
        fake.store.stopping && fake.store.running,
        "the draining store is not killed or stopped early"
    );
    assert!(
        fake.store.queued.len() == 1 && fake.store.queued[0].1.is_none(),
        "only the queued put waits"
    );
    fake.exit();
    fake.start();
    assert!(fake.store.available && fake.writes.len() == 2 && fake.op(1) == "put");
    assert!(fake.store.owed() == 0 && fake.kills == 0);
}

#[test]
fn a_store_that_ignores_its_input_ending_is_killed_at_the_hang_bound() {
    let mut fake = Fake::new();
    fake.put("<svg/>");
    fake.start();
    assert!(fake.stop());
    fake.get(14);
    fake.put("<svg>2</svg>");
    fake.now = DRAIN_MS + 1;
    fake.tick();
    assert!(
        fake.answers.len() == 1 && fake.kills == 0,
        "a miss at drainMs, before any kill"
    );
    fake.now = DRAIN_HANG_MS;
    fake.tick();
    assert!(
        fake.kills == 1 && fake.store.hang_kills == 1,
        "the drain hang bound kills the store"
    );
    assert!(
        fake.store.available && fake.store.queued.len() == 1 && fake.store.queued[0].1.is_none()
    );
    fake.exit();
    assert!(
        fake.store.available
            && fake.store.starting
            && !fake.store.stopping
            && fake.store.exits == 1
    );
    fake.start();
    assert!(
        fake.writes.len() == 2 && fake.op(1) == "put" && fake.store.owed() == 0 && fake.kills == 1
    );
    fake.get(16);
    fake.reply("{\"id\":16,\"miss\":true}");
    assert!(fake.answers.len() == 2 && fake.answers[1].0 == 16 && fake.store.misses == 1);
}

#[test]
fn a_failed_spawn_latches_off_and_garbled_lines_are_ignored() {
    let mut fake = Fake::new();
    fake.get(17);
    assert!(
        fake.store.starting && fake.store.active(),
        "a get starts the store"
    );
    let events = fake.store.spawn_failed(fake.now);
    fake.take(events);
    assert!(
        !fake.store.available
            && fake.answers == vec![(17, String::new())]
            && fake.store.owed() == 0
    );
    assert!(
        !fake.store.active(),
        "the failed store no longer reads as active"
    );
    let mut fake = Fake::new();
    fake.ask(11);
    fake.start();
    fake.reply("null");
    fake.reply("not json");
    assert!(
        fake.store.owed() == 1 && fake.store.available,
        "a null or garbled line is ignored"
    );
}
