use std::cell::Cell;
use std::hash::{Hash, Hasher};

thread_local! {
    static WORK: Cell<(u64, u64)> = const { Cell::new((0, u64::MAX)) };
}

fn charge(bytes: usize) {
    WORK.with(|work| {
        let (used, limit) = work.get();
        let next = used + bytes as u64 + 1;
        assert!(
            next <= limit,
            "JSON key work exceeded its budget: {next} > {limit}"
        );
        work.set((next, limit));
    });
}

pub(crate) fn compare(left: &str, right: &str) {
    charge(left.len().min(right.len()));
}

struct Restore((u64, u64));

impl Drop for Restore {
    fn drop(&mut self) {
        WORK.with(|work| work.set(self.0));
    }
}

pub(crate) fn measured<T>(limit: u64, body: impl FnOnce() -> T) -> (T, u64) {
    let restore = Restore(WORK.with(|work| work.replace((0, limit))));
    let result = body();
    let used = WORK.with(|work| work.get().0);
    drop(restore);
    (result, used)
}

// This thread's CPU time, which run-queue delay on a loaded host does not add to.
pub(crate) fn thread_cpu() -> std::time::Duration {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `now` is a valid timespec that outlives the call.
    let status = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut now) };
    assert_eq!(status, 0, "clock_gettime(CLOCK_THREAD_CPUTIME_ID) failed");
    std::time::Duration::new(now.tv_sec as u64, now.tv_nsec as u32)
}

pub(crate) struct Name(String);

impl From<String> for Name {
    fn from(text: String) -> Self {
        charge(text.len());
        Self(text)
    }
}

impl From<Name> for String {
    fn from(name: Name) -> Self {
        name.0
    }
}

impl Clone for Name {
    fn clone(&self) -> Self {
        charge(self.0.len());
        Self(self.0.clone())
    }
}

impl PartialEq for Name {
    fn eq(&self, other: &Self) -> bool {
        compare(&self.0, &other.0);
        self.0 == other.0
    }
}

impl Eq for Name {}

impl Hash for Name {
    fn hash<H: Hasher>(&self, state: &mut H) {
        charge(self.0.len());
        self.0.hash(state);
    }
}

#[test]
fn key_work_budget_interrupts_parsing_and_restores_the_meter() {
    let input = r#"{"first":1,"second":2}"#;
    assert!(std::panic::catch_unwind(|| measured(0, || crate::jsondoc::parse(input))).is_err());
    assert!(crate::jsondoc::parse(input).is_ok());
}
