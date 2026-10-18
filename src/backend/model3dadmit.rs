use super::codereq::{Cancellation, Ticket};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

const POLL_STEP: Duration = Duration::from_millis(5);

struct State { busy: bool, foreground: usize, background: Option<Cancellation> }

pub(super) struct Gate { state: Mutex<State>, wake: Condvar }

pub(super) struct Guard<'a>(&'a Gate);

impl Gate {
    pub(super) const fn new() -> Self {
        Self { state: Mutex::new(State { busy: false, foreground: 0, background: None }), wake: Condvar::new() }
    }

    pub(super) fn enter(&self, ticket: &Ticket, background: bool, deadline: Instant) -> Result<Guard<'_>,String> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !background {
            state.foreground += 1;
            let cancellation = state.background.take();
            drop(state);
            if let Some(cancellation) = cancellation { cancellation.cancel(); }
            state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        }
        loop {
            let refusal = if ticket.superseded() { Some("preview superseded") }
                else if Instant::now() >= deadline { Some("3D preview timed out") } else { None };
            if let Some(reason) = refusal {
                if !background { state.foreground -= 1; }
                self.wake.notify_all();
                return Err(reason.into());
            }
            if !state.busy && (!background || state.foreground == 0) {
                if !background { state.foreground -= 1; }
                state.busy = true;
                if background { state.background = Some(ticket.cancellation()); }
                return Ok(Guard(self));
            }
            state = self.wake.wait_timeout(state,POLL_STEP.min(deadline.saturating_duration_since(Instant::now())))
                .unwrap_or_else(|e| e.into_inner()).0;
        }
    }
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.busy = false;
        state.background = None;
        self.0.wake.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{codereq, jail};
    use std::sync::{mpsc, Arc};

    const PROBE_WAIT: Duration = Duration::from_millis(50);
    const PROBE_LIMIT: Duration = Duration::from_secs(2);

    // One enter on its own thread, so a waiter that ignores its deadline fails the test instead of hanging it.
    fn probe(gate: &Arc<Gate>, background: bool) -> (Result<(),String>,Duration) {
        let gate = Arc::clone(gate);
        let (sent, got) = mpsc::channel();
        std::thread::spawn(move || {
            let ticket = codereq::begin("model3d-admission-probe");
            let began = Instant::now();
            let result = gate.enter(&ticket,background,began+PROBE_WAIT).map(|_| ());
            sent.send((result,began.elapsed())).unwrap();
            ticket.discard();
        });
        got.recv_timeout(PROBE_LIMIT).expect("a waiter outlived its deadline")
    }

    #[test]
    fn foreground_cancels_a_running_background_parser_before_admission() {
        let gate = Arc::new(Gate::new());
        let background = Arc::clone(&gate);
        let (started, ready) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let ticket = codereq::begin("model3d-priority-background");
            let _guard = background.enter(&ticket,true,Instant::now()+Duration::from_secs(10)).unwrap();
            let child = jail::spawn_jailed(&["/bin/sleep".into(),"5".into()], |cmd| { cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()); }).unwrap();
            assert!(ticket.attach(child));
            started.send(()).unwrap();
            assert!(ticket.reap_until(Some(Instant::now()+Duration::from_secs(10))).is_none());
            assert!(ticket.superseded());
            ticket.discard();
        });
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        let ticket = codereq::begin("model3d-priority-foreground");
        let began = Instant::now();
        let guard = gate.enter(&ticket,false,began+Duration::from_secs(3)).unwrap();
        assert!(began.elapsed() < Duration::from_secs(3));
        drop(guard);
        ticket.discard();
        worker.join().unwrap();
    }

    #[test]
    fn foreground_waiters_precede_background_and_keep_a_deadline() {
        let gate = Arc::new(Gate::new());
        let held_ticket = codereq::begin("model3d-admission-held");
        let held = gate.enter(&held_ticket,false,Instant::now()+Duration::from_secs(2)).unwrap();
        for background in [true,false] {
            let (result,waited) = probe(&gate,background);
            assert_eq!(result,Err("3D preview timed out".into()),"background={background}: a busy gate admitted a waiter");
            assert!(waited >= PROBE_WAIT,"background={background}: refused after {waited:?}, before its deadline");
        }
        assert_eq!(gate.state.lock().unwrap().foreground,0);
        let foreground = Arc::clone(&gate);
        let (entered, next) = mpsc::channel();
        let announced = entered.clone();
        let worker = std::thread::spawn(move || {
            let ticket = codereq::begin("model3d-admission-next");
            let _guard = foreground.enter(&ticket,false,Instant::now()+Duration::from_secs(2)).unwrap();
            announced.send("foreground").unwrap();
            ticket.discard();
        });
        let deadline = Instant::now()+Duration::from_secs(1);
        while gate.state.lock().unwrap().foreground == 0 {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let background = Arc::clone(&gate);
        let behind = std::thread::spawn(move || {
            let ticket = codereq::begin("model3d-admission-behind");
            let _guard = background.enter(&ticket,true,Instant::now()+Duration::from_secs(2)).unwrap();
            entered.send("background").unwrap();
            ticket.discard();
        });
        drop(held);
        held_ticket.discard();
        assert_eq!(next.recv_timeout(Duration::from_secs(2)).unwrap(),"foreground");
        assert_eq!(next.recv_timeout(Duration::from_secs(2)).unwrap(),"background");
        worker.join().unwrap();
        behind.join().unwrap();
        assert_eq!(probe(&gate,true).0,Ok(()));
    }

    #[test]
    fn a_counted_foreground_waiter_keeps_a_free_gate_from_background() {
        let gate = Gate::new();
        let ticket = codereq::begin("model3d-admission-pending");
        gate.state.lock().unwrap().foreground = 1;
        let refused = gate.enter(&ticket,true,Instant::now()+PROBE_WAIT).map(|_| ());
        assert_eq!(refused,Err("3D preview timed out".into()),"a background parser was admitted ahead of a waiting foreground preview");
        gate.state.lock().unwrap().foreground = 0;
        assert!(gate.enter(&ticket,true,Instant::now()+PROBE_WAIT).is_ok());
        ticket.discard();
    }
}
