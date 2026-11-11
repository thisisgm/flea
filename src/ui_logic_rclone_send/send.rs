//! One send's state as a front end holds it, driven by the parsed replies.
use super::wire::{Event, Job};

/// Where one send stands for its front end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Asked for, no job id yet.
    Asked,
    /// Refused before a job started; the backend's sentence is a diagnostic.
    Refused(String),
    /// Waiting or copying.
    Sending,
    /// rclone check is running; true when it compares checksums.
    Checking(bool),
    /// The job failed; Retry and Dismiss are offered.
    Failed(String),
    /// The send ended and left a receipt: checked by checksum or size, or not checked.
    Receipt { check: String, note: String },
    /// Cancelled before anything was copied, or dismissed: nothing to show.
    Gone,
    /// The backend ended while the send ran; its job id means nothing to the next backend.
    Lost,
}

/// One send as a front end holds it: its own request, the job the backend gave it and that job's newest row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Send {
    pub request: u64,
    pub job: Option<u64>,
    pub remote: String,
    pub folder: String,
    pub phase: Phase,
    pub row: Job,
    /// Set by Cancel until the job ends, so a second press sends nothing.
    pub cancelling: bool,
    /// Set by Retry until the backend's next row or its refusal; the send stays failed meanwhile.
    pub retrying: bool,
}

impl Send {
    pub fn new(request: u64, remote: &str, folder: &str, items: usize) -> Self {
        Self {
            request,
            job: None,
            remote: remote.into(),
            folder: folder.into(),
            phase: Phase::Asked,
            row: Job {
                total: items,
                ..Job::default()
            },
            cancelling: false,
            retrying: false,
        }
    }

    /// True while the card shows a live block with Cancel.
    pub fn running(&self) -> bool {
        matches!(
            self.phase,
            Phase::Asked | Phase::Sending | Phase::Checking(_)
        )
    }

    /// Applies one parsed event; anything for another request or job, out of order, or after the end changes nothing.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::Started {
                request,
                job,
                error,
            } if *request == self.request && self.phase == Phase::Asked => {
                match job {
                    Some(job) => {
                        self.job = Some(*job);
                        self.phase = Phase::Sending;
                    }
                    None => self.phase = Phase::Refused(error.clone()),
                }
                true
            }
            Event::Jobs { jobs, snapshot } => {
                match (self.job, jobs.iter().find(|row| Some(row.id) == self.job)) {
                    (Some(_), Some(row)) => self.row_update(row),
                    // A snapshot without this send's row means it was dismissed or replaced; the card lets it go.
                    (Some(_), None) if *snapshot && matches!(self.phase, Phase::Failed(_)) => {
                        self.phase = Phase::Gone;
                        true
                    }
                    _ => false,
                }
            }
            Event::Sent(outcome)
                if Some(outcome.id) == self.job
                    && outcome.request == self.request
                    && !matches!(self.phase, Phase::Receipt { .. } | Phase::Gone) =>
            {
                self.cancelling = false;
                self.phase = if outcome.receipt {
                    Phase::Receipt {
                        check: outcome.check.clone(),
                        note: outcome.note.clone(),
                    }
                } else if outcome.ok || outcome.cancelled {
                    Phase::Gone
                } else {
                    Phase::Failed(if outcome.error.is_empty() {
                        self.row.reason.clone()
                    } else {
                        outcome.error.clone()
                    })
                };
                true
            }
            Event::Refused { job, .. } if Some(*job) == self.job && self.cancelling => {
                self.cancelling = false;
                true
            }
            // A refused retry leaves the failed send as it was, with the backend's newer sentence.
            Event::Refused { job, error } if Some(*job) == self.job && self.retrying => {
                self.retrying = false;
                if !error.is_empty() {
                    self.phase = Phase::Failed(error.clone());
                }
                true
            }
            _ => false,
        }
    }

    fn row_update(&mut self, row: &Job) -> bool {
        if row.kind != "send"
            || matches!(
                self.phase,
                Phase::Receipt { .. } | Phase::Gone | Phase::Lost
            )
        {
            return false;
        }
        let phase = match (row.state.as_str(), row.phase.as_str()) {
            // A snapshot written before the backend read the retry still says failed; the retry's own row or refusal decides.
            ("failed", _) if self.retrying => return false,
            ("failed", _) => Phase::Failed(row.reason.clone()),
            // A finished job waits for rclonesent, which says what the status bar keeps.
            ("done", _) => self.phase.clone(),
            ("waiting" | "running", "checking") => Phase::Checking(row.check != "size"),
            // Back to sending only on a retry's new attempt; a checking job never steps back.
            ("waiting" | "running", _) if !matches!(self.phase, Phase::Checking(_)) => {
                Phase::Sending
            }
            _ => return false,
        };
        if matches!(phase, Phase::Failed(_)) {
            self.cancelling = false;
        }
        self.retrying = false;
        self.phase = phase;
        self.row = row.clone();
        true
    }

    /// Cancel is live while sending and while checking; the jobs line to send, once.
    pub fn cancel(&mut self) -> Option<String> {
        let job = self.job?;
        if !self.running() || self.cancelling {
            return None;
        }
        self.cancelling = true;
        Some(super::jobs_line("cancel", job))
    }

    /// Retry only a failed job, once until the backend answers; it stays failed until the job's next row.
    pub fn retry(&mut self) -> Option<String> {
        let job = self.job?;
        if !matches!(self.phase, Phase::Failed(_)) || self.retrying {
            return None;
        }
        self.retrying = true;
        Some(super::jobs_line("retry", job))
    }

    /// Dismiss a failed job; a lost one leaves without a request, and a retry in flight is not dismissed.
    pub fn dismiss(&mut self) -> Option<String> {
        let line = match (&self.phase, self.job) {
            (Phase::Failed(_), Some(_)) if self.retrying => return None,
            (Phase::Failed(_), Some(job)) => Some(super::jobs_line("dismiss", job)),
            (Phase::Lost | Phase::Refused(_), _) => None,
            _ => return None,
        };
        self.phase = Phase::Gone;
        line
    }

    /// The backend ended: its job ids restart in the next one, so every send drops its id and an unfinished one is lost.
    pub fn backend_ended(&mut self) -> bool {
        let unfinished = self.running() || matches!(self.phase, Phase::Failed(_));
        self.job = None;
        self.cancelling = false;
        self.retrying = false;
        if unfinished {
            self.phase = Phase::Lost;
        }
        // True only when the block changes; a finished send's receipt reads the same without its id.
        unfinished
    }
}
