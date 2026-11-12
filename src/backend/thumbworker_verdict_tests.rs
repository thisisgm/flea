use super::*;

const CHILD_WAIT_MS: c_int = 5000;

extern "C" {
    fn getpid() -> c_int;
    fn pause() -> c_int;
}

// How a job's child ends in the verdict test.
enum Ends {
    Exits(i32),
    Dies,
    Hangs,
}

// Forks a child that ends the given way, waits until it has ended without reaping it, and hands finish() the job.
fn job_that(ends: &Ends, reply: OwnedFd) -> Running {
    let pid = unsafe { fork() };
    if pid == 0 {
        match ends {
            Ends::Exits(code) => unsafe { _exit(*code) },
            Ends::Dies => drop(crate::procsignal::signal_process(
                unsafe { getpid() },
                SIGKILL,
            )),
            Ends::Hangs => loop {
                unsafe { pause() };
            },
        };
        unsafe { _exit(CHILD_OK) };
    }
    assert!(pid > 0, "the test could not fork");
    let pidfd = unsafe { OwnedFd::from_raw_fd(pidfd_open(pid, 0)) };
    if !matches!(ends, Ends::Hangs) {
        let mut ended = PollFd {
            fd: pidfd.as_raw_fd(),
            events: POLLIN,
            revents: 0,
        };
        assert_eq!(
            unsafe { poll(&mut ended, 1, CHILD_WAIT_MS) },
            1,
            "the child never ended"
        );
    }
    Running {
        pid,
        pidfd,
        reply,
        deadline: Instant::now(),
        stop: None,
    }
}

#[test]
fn a_verdict_is_read_from_how_the_child_ended() {
    let cases = [
        (Ends::Exits(CHILD_OK), false, SUCCEEDED),
        (Ends::Exits(CHILD_REFUSED), false, FAILED),
        (Ends::Exits(CHILD_MACHINE), false, NOT_STARTED),
        (Ends::Dies, false, FAILED),
        (Ends::Exits(CHILD_OK), true, FAILED),
        (Ends::Hangs, true, FAILED),
    ];
    for (ends, past_deadline, verdict) in cases {
        let (mine, theirs) = fdpass::pair().unwrap();
        let mut job = job_that(&ends, mine);
        if past_deadline {
            job.stop(jobs::Stop::Deadline);
        }
        finish(job);
        let answer = fdpass::recv(theirs.as_raw_fd())
            .unwrap()
            .expect("a verdict");
        assert_eq!(
            answer.payload,
            vec![verdict],
            "past deadline {} wanted {}",
            past_deadline,
            verdict as char
        );
    }
}
