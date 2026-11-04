//! One table of checks per ported source file; each check names the source line it ports, so a row maps to its evidence.
use std::fmt::Debug;

pub struct Checks {
    source: &'static str,
    failures: Vec<String>,
    ran: usize,
}

impl Checks {
    pub fn new(source: &'static str) -> Self {
        Self {
            source,
            failures: Vec::new(),
            ran: 0,
        }
    }

    pub fn eq<T: PartialEq + Debug>(&mut self, line: u32, label: &str, got: T, want: T) {
        self.ran += 1;
        if got != want {
            self.failures.push(format!(
                "{}:{line} {label}: got {got:?}, want {want:?}",
                self.source
            ));
        }
    }

    pub fn ok(&mut self, line: u32, label: &str, holds: bool) {
        self.ran += 1;
        if !holds {
            self.failures
                .push(format!("{}:{line} {label}", self.source));
        }
    }

    /// Lines the source spends driving or collecting its checks, which this test's own steps and finish replace.
    pub fn covers(&mut self, lines: &[u32]) {
        let _ = lines;
    }

    /// Fails the test with every failed check listed, and a table that ran nothing fails too.
    pub fn finish(self) {
        assert!(self.ran > 0, "{} ran no checks", self.source);
        assert!(
            self.failures.is_empty(),
            "{} failed:\n{}",
            self.source,
            self.failures.join("\n")
        );
    }
}
