use super::*;

struct Running {
    pid: c_int,
    pidfd: OwnedFd,
    reply: OwnedFd,
    deadline: Instant,
    stop: Option<Stop>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stop {
    Deadline,
    Cancelled,
}

impl Running {
    /// Record the stop reason and signal the decoder child.
    fn request_stop(&mut self, stop: Stop) {
        self.stop = Some(stop);
        let _ = crate::procsignal::signal_process(self.pid, SIGKILL);
    }

    /// Poll exit and control while a decoder is live, then wait only for exit.
    fn poll_fds(&self) -> [PollFd; 2] {
        [
            PollFd {
                fd: self.pidfd.as_raw_fd(),
                events: POLLIN,
                revents: 0,
            },
            PollFd {
                fd: if self.stop.is_none() {
                    self.reply.as_raw_fd()
                } else {
                    -1
                },
                events: POLLIN,
                revents: 0,
            },
        ]
    }
}

// Reap only after the pidfd says the child exited. A decoder waiting on I/O
// can hold its own slot after SIGKILL without holding up the other jobs.
/// Reap an exited decoder and report the final job verdict.
fn finish(job: Running) {
    let mut status: c_int = 0;
    let reaped = loop {
        let pid = unsafe { waitpid(job.pid, &mut status, 0) };
        if pid == job.pid {
            break true;
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(EINTR) {
            break false;
        }
    };
    let verdict = if !reaped {
        NOT_STARTED
    } else if job.stop == Some(Stop::Cancelled) {
        VIDEO_CANCELLED
    } else {
        match std::process::ExitStatus::from_raw(status).code() {
            Some(CHILD_OK) if job.stop.is_none() => SUCCEEDED,
            Some(CHILD_MACHINE) => NOT_STARTED,
            _ => FAILED,
        }
    };
    // corner: a backend that gave up on this job has closed its end, and there is no one left to tell.
    let _ = fdpass::send_byte(&job.reply, verdict);
}

// Sample request: payload [0, 1, 0, 0, 1] (size 256, film strip on), descriptors [input, output, reply].
/// Validate a worker request and start its confined decoder child.
fn start(message: fdpass::Received, decoder: &Decoder, abi: i64, running: &mut Vec<Running>) {
    let mut fds = message.fds.into_iter();
    let (Some(input), Some(output), Some(reply), None) =
        (fds.next(), fds.next(), fds.next(), fds.next())
    else {
        return;
    };
    let payload = message.payload;
    let size = if payload.len() == REQUEST_BYTES {
        u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]])
    } else {
        0
    };
    if size == 0 || size > MAX_SIZE {
        let _ = fdpass::send_byte(&reply, NOT_STARTED);
        return;
    }
    let film_strip = payload[4] != 0;
    let pid = unsafe { fork() };
    if pid == 0 {
        child(
            input.as_raw_fd(),
            output.as_raw_fd(),
            size as c_int,
            film_strip,
            decoder,
            abi,
        );
    }
    if pid < 0 {
        let _ = fdpass::send_byte(&reply, NOT_STARTED);
        return;
    }
    drop(input);
    drop(output);
    let raw = unsafe { pidfd_open(pid, 0) };
    if raw < 0 {
        // corner: with no descriptor there is no deadline to enforce, so the child is ended now and judges nothing.
        let _ = crate::procsignal::signal_process(pid, SIGKILL);
        let mut status: c_int = 0;
        unsafe { waitpid(pid, &mut status, 0) };
        let _ = fdpass::send_byte(&reply, NOT_STARTED);
        return;
    }
    let pidfd = unsafe { OwnedFd::from_raw_fd(raw) };
    running.push(Running {
        pid,
        pidfd,
        reply,
        deadline: Instant::now() + JOB_TIMEOUT,
        stop: None,
    });
}

// Milliseconds to the nearest deadline, rounded up, or -1 to wait for a request with no child running.
/// Wait until the earliest deadline among live decoder jobs.
fn poll_timeout(running: &[Running]) -> c_int {
    match running
        .iter()
        .filter(|job| job.stop.is_none())
        .map(|job| job.deadline)
        .min()
    {
        None => -1,
        Some(at) => at
            .saturating_duration_since(Instant::now())
            .as_millis()
            .saturating_add(1)
            .min(c_int::MAX as u128) as c_int,
    }
}

/// Handle controls, reap completed decoders and stop expired jobs.
fn update_jobs(running: Vec<Running>, polled: &[PollFd]) -> Vec<Running> {
    let now = Instant::now();
    let mut still = Vec::with_capacity(running.len());
    for (mut job, polled) in running.into_iter().zip(polled.chunks_exact(2)) {
        let cancelled = polled[1].revents != 0
            && match fdpass::recv(job.reply.as_raw_fd()) {
                Ok(None) => true,
                Ok(Some(message)) => message.payload == [CANCEL_VIDEO] && message.fds.is_empty(),
                Err(_) => false,
            };
        if cancelled {
            job.request_stop(Stop::Cancelled);
        }
        if polled[0].revents & POLLIN != 0 {
            finish(job);
        } else {
            if job.stop.is_none() && now >= job.deadline {
                job.request_stop(Stop::Deadline);
            }
            still.push(job);
        }
    }
    still
}

/// Poll requests and in-flight decoder jobs until the backend disconnects.
pub(super) fn serve(requests: &OwnedFd, decoder: &Decoder, abi: i64) -> i32 {
    let mut running: Vec<Running> = Vec::new();
    loop {
        let mut fds: Vec<PollFd> = Vec::with_capacity(running.len() * 2 + 1);
        fds.push(PollFd {
            fd: requests.as_raw_fd(),
            events: POLLIN,
            revents: 0,
        });
        fds.extend(running.iter().flat_map(Running::poll_fds));
        let ready = unsafe { poll(fds.as_mut_ptr(), fds.len(), poll_timeout(&running)) };
        if ready < 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(EINTR) {
                continue;
            }
            return 1;
        }
        // Children first, so a verdict is never held behind a request.
        running = update_jobs(running, &fds[1..]);
        if fds[0].revents != 0 {
            match fdpass::recv(requests.as_raw_fd()) {
                Ok(Some(message)) => start(message, decoder, abi, &mut running),
                // The backend closed its end, so there will be no more work; bwrap's own init ends any child still running.
                Ok(None) => return 0,
                Err(e) if e.raw_os_error() == Some(EINTR) => {}
                // A malformed packet is dropped with its descriptors; anything else is a socket that no longer works.
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {}
                Err(_) => return 1,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A child that ends at once is readable on its pidfd long before this; it bounds a broken test, it times nothing.
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

    // Fork a child that ends in the requested way.
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
        Running {
            pid,
            pidfd,
            reply,
            deadline: Instant::now(),
            stop: None,
        }
    }

    fn wait_for_exit(job: &Running) -> PollFd {
        let [mut exited, _] = job.poll_fds();
        assert_eq!(
            unsafe { poll(&mut exited, 1, CHILD_WAIT_MS) },
            1,
            "the child never ended"
        );
        exited
    }

    /// Verify that a stopped decoder cannot delay another job's reply.
    #[test]
    fn a_cancelled_child_waits_for_its_pidfd_while_another_job_reports() {
        let (reply, controller) = fdpass::pair().unwrap();
        let cancelled = job_that(&Ends::Hangs, reply);
        let cancelled_pid = cancelled.pid;
        let (reply, observer) = fdpass::pair().unwrap();
        let successful = job_that(&Ends::Exits(CHILD_OK), reply);
        wait_for_exit(&successful);
        fdpass::send_byte(&controller, CANCEL_VIDEO).unwrap();

        let running = vec![cancelled, successful];
        let mut polled: Vec<_> = running.iter().flat_map(Running::poll_fds).collect();
        assert_eq!(
            unsafe { poll(polled.as_mut_ptr(), polled.len(), CHILD_WAIT_MS) },
            2
        );
        assert_eq!(
            polled[0].revents, 0,
            "the cancelled child's pidfd is not ready"
        );
        let waiting = update_jobs(running, &polled);
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].pid, cancelled_pid);
        assert!(matches!(waiting[0].stop, Some(Stop::Cancelled)));
        let answer = fdpass::recv(observer.as_raw_fd()).unwrap().unwrap();
        assert_eq!(answer.payload, [SUCCEEDED]);

        let [_, control] = waiting[0].poll_fds();
        assert_eq!(control.fd, -1, "a stopped job only waits for its pidfd");
        let exited = wait_for_exit(&waiting[0]);
        assert!(update_jobs(waiting, &[exited, control]).is_empty());
        let answer = fdpass::recv(controller.as_raw_fd()).unwrap().unwrap();
        assert_eq!(answer.payload, [VIDEO_CANCELLED]);
    }

    /// A live child past its deadline is stopped by the normal poll pass.
    #[test]
    fn an_expired_job_stops_without_a_cancel_message() {
        let (mine, theirs) = fdpass::pair().unwrap();
        let mut job = job_that(&Ends::Hangs, mine);
        job.deadline = Instant::now();
        let polled = job.poll_fds();
        let waiting = update_jobs(vec![job], &polled);
        assert_eq!(waiting.len(), 1);
        assert!(matches!(waiting[0].stop, Some(Stop::Deadline)));
        let exited = wait_for_exit(&waiting[0]);
        let [_, control] = waiting[0].poll_fds();
        assert!(update_jobs(waiting, &[exited, control]).is_empty());
        assert_eq!(fdpass::recv(theirs.as_raw_fd()).unwrap().unwrap().payload, [FAILED]);
    }

    /// Classify completed children by their exit or by the reason they were stopped.
    #[test]
    fn a_verdict_is_read_from_how_the_child_ended() {
        let cases = [
            (Ends::Exits(CHILD_OK), None, SUCCEEDED),
            (Ends::Exits(CHILD_REFUSED), None, FAILED),
            (Ends::Exits(CHILD_MACHINE), None, NOT_STARTED),
            (Ends::Dies, None, FAILED),
            (Ends::Exits(CHILD_OK), Some(Stop::Deadline), FAILED),
            (Ends::Hangs, Some(Stop::Deadline), FAILED),
            (Ends::Hangs, Some(Stop::Cancelled), VIDEO_CANCELLED),
        ];
        for (ends, stop, verdict) in cases {
            let (mine, theirs) = fdpass::pair().unwrap();
            let mut job = job_that(&ends, mine);
            if let Some(stop) = stop {
                job.request_stop(stop);
                assert_eq!(poll_timeout(std::slice::from_ref(&job)), -1);
            }
            wait_for_exit(&job);
            finish(job);
            let answer = fdpass::recv(theirs.as_raw_fd())
                .unwrap()
                .expect("a verdict");
            assert_eq!(answer.payload, vec![verdict]);
        }
    }
}
