use super::*;

pub(super) struct Running {
    pub(super) pid: c_int,
    pub(super) pidfd: OwnedFd,
    pub(super) reply: OwnedFd,
    pub(super) deadline: Instant,
    pub(super) stop: Option<Stop>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stop {
    Cancelled,
    Deadline,
}

impl Running {
    pub(super) fn stop(&mut self, reason: Stop) {
        self.stop = Some(reason);
        let _ = crate::procsignal::signal_process(self.pid, SIGKILL);
    }
}

// Reaps one child and answers its job; a child killed by a signal, the deadline's included, ran on the file and failed.
pub(super) fn finish(job: Running) {
    let mut status: c_int = 0;
    let reaped = loop {
        let result = unsafe { waitpid(job.pid, &mut status, 0) };
        if result == job.pid {
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
fn poll_timeout(running: &[Running]) -> c_int {
    match running
        .iter()
        .filter(|r| r.stop.is_none())
        .map(|r| r.deadline)
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

pub(super) fn serve(requests: &OwnedFd, decoder: &Decoder, abi: i64) -> i32 {
    let mut running: Vec<Running> = Vec::new();
    loop {
        let mut fds: Vec<PollFd> = Vec::with_capacity(running.len() * 2 + 1);
        fds.push(PollFd {
            fd: requests.as_raw_fd(),
            events: POLLIN,
            revents: 0,
        });
        for job in &running {
            fds.push(PollFd {
                fd: job.pidfd.as_raw_fd(),
                events: POLLIN,
                revents: 0,
            });
            fds.push(PollFd {
                fd: if job.stop.is_none() {
                    job.reply.as_raw_fd()
                } else {
                    -1
                },
                events: POLLIN,
                revents: 0,
            });
        }
        let ready = unsafe { poll(fds.as_mut_ptr(), fds.len(), poll_timeout(&running)) };
        if ready < 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(EINTR) {
                continue;
            }
            return 1;
        }
        // Children first, so a verdict is never held behind a request.
        let now = Instant::now();
        let mut still = Vec::with_capacity(running.len());
        for (mut job, polled) in running.into_iter().zip(fds[1..].chunks_exact(2)) {
            if polled[1].revents != 0 {
                let cancel = match fdpass::recv(job.reply.as_raw_fd()) {
                    Ok(None) => true,
                    Ok(Some(message)) => {
                        message.payload == [CANCEL_VIDEO] && message.fds.is_empty()
                    }
                    Err(_) => false,
                };
                if cancel {
                    job.stop(Stop::Cancelled);
                }
            }
            if polled[0].revents & POLLIN != 0 {
                finish(job);
            } else {
                if job.stop.is_none() && now >= job.deadline {
                    job.stop(Stop::Deadline);
                }
                still.push(job);
            }
        }
        running = still;
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
