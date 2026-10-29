//! The running flash: the backend answers a press lands in.
use super::{Flash, Run, MS_PER_SECOND, RATE_WINDOW_MS};
use crate::g2::op2::{cards::JobBlock, Effect};
use flea::i18n::I18n;
use flea::ui_logic_op2::{cards as logic, flash, flashcard, flashendings, flashpane, wire};

impl Flash {
    pub fn open_with_image(&mut self, path: &str) -> Vec<Effect> {
        if self.running() || self.runner.press_sent {
            return self.open();
        }
        self.image = Some(flash::Image {
            path: path.into(),
            ..flash::Image::default()
        });
        self.pending_drive.clear();
        self.chosen = None;
        self.cursor = 0;
        self.open();
        vec![
            Effect::Send(wire::flash_image_request(path)),
            Effect::Send(wire::flash_targets_request(path)),
        ]
    }

    pub fn open_with_drive(&mut self, device: &str) -> Vec<Effect> {
        if self.running() || self.runner.press_sent {
            return self.open();
        }
        self.pending_drive = device.into();
        self.chosen = None;
        self.cursor = 0;
        self.open();
        let image = self
            .image
            .as_ref()
            .map(|image| image.path.clone())
            .unwrap_or_default();
        if !image.is_empty() {
            return vec![
                Effect::Send(wire::flash_image_request(&image)),
                Effect::Send(wire::flash_targets_request(&image)),
            ];
        }
        vec![Effect::Send(wire::flash_targets_request(""))]
    }

    pub fn receive_image(&mut self, path: &str, reply: &wire::FlashImage) {
        let Some(current) = self.image.as_ref() else {
            return;
        };
        if !self.shown && !self.running() {
            return;
        }
        if let Some(image) = flash::image_from(
            Some(current),
            path,
            &reply.kind,
            reply.bytes,
            reply.unpacked,
            reply.known,
        ) {
            self.image = Some(image);
        }
    }

    pub fn receive_targets(&mut self, rows: Vec<wire::TargetRow>) {
        if !self.shown && !self.running() {
            return;
        }
        let drives: Vec<flash::Drive> = rows
            .into_iter()
            .map(|row| flash::Drive {
                dev: row.dev,
                seq: row.seq,
                serial: row.serial,
                label: row.label,
                model: row.model,
                bytes: row.bytes,
                refused: row.refused,
            })
            .collect();
        let was = std::mem::replace(&mut self.drives, drives);
        let first = !self.drives_ready;
        self.drives_ready = true;
        if self.running() {
            return;
        }
        // A fitting drive plugged in after Eject brings the Flash row back.
        if self.ejected {
            if !self.drives.iter().any(flash::fits) {
                return;
            }
            self.ejected = false;
            self.runner.ending = None;
        }
        let next = flashpane::reconcile(
            &flashpane::Poll {
                chosen: self.chosen.clone(),
                held: self.held,
                cursor: self.cursor,
                drives: was,
                first,
                pending_disk: if self.pending_drive.is_empty() {
                    None
                } else {
                    Some(self.pending_drive.clone())
                },
            },
            &self.drives,
        );
        let mut mark = flashpane::ViewMark {
            pending_drive: self.pending_drive.clone(),
            chosen: self.chosen.clone(),
            held: self.held,
            cursor: self.cursor,
        };
        flashpane::land(&mut mark, &next);
        self.pending_drive = mark.pending_drive;
        self.chosen = mark.chosen;
        self.held = mark.held;
        self.cursor = mark.cursor;
    }

    pub fn start_transfer(&mut self, i18n: &I18n, id: u64, total: u64, known: bool, now_ms: u64) {
        let drive = self
            .chosen_drive()
            .map(|drive| flash::drive_name(drive, &self.drives))
            .unwrap_or_default();
        let name = self
            .image
            .as_ref()
            .map(|image| logic::leaf_of(&image.path))
            .unwrap_or_default();
        self.runner.press_sent = false;
        self.runner.rate = 0;
        self.runner.rate_at = now_ms;
        self.runner.rate_bytes = 0;
        let transfer = flashcard::Transfer {
            phase: "prepare".into(),
            drive,
            name,
            bytes: 0,
            total,
            known,
            permille: 0,
        };
        self.runner.transfer = Some(Run {
            id,
            drawn: flashcard::render(i18n, &transfer, 0),
            transfer,
        });
        self.runner.ending = None;
        self.cursor = self.flash_row();
    }

    fn note_rate(&mut self, bytes: u64, now_ms: u64) {
        // Verify counts again from its first chunk, so a count that goes back restarts the window there.
        if bytes < self.runner.rate_bytes {
            self.runner.rate_at = now_ms;
            self.runner.rate_bytes = bytes;
            self.runner.rate = 0;
            return;
        }
        if now_ms.saturating_sub(self.runner.rate_at) >= RATE_WINDOW_MS
            && bytes >= self.runner.rate_bytes
        {
            // The guard above proves the span is at least RATE_WINDOW_MS, so the division is safe.
            let span = now_ms - self.runner.rate_at;
            self.runner.rate = bytes
                .saturating_sub(self.runner.rate_bytes)
                .saturating_mul(MS_PER_SECOND)
                / span;
            self.runner.rate_at = now_ms;
            self.runner.rate_bytes = bytes;
        }
    }

    pub fn progress_transfer(
        &mut self,
        i18n: &I18n,
        sample: &wire::FlashProgress,
        now_ms: u64,
    ) -> Vec<Effect> {
        let Some(id) = self.runner.transfer.as_ref().map(|run| run.id) else {
            return Vec::new();
        };
        if id != sample.id {
            return Vec::new();
        }
        self.note_rate(sample.bytes, now_ms);
        let Some(run) = self.runner.transfer.as_mut() else {
            return Vec::new();
        };
        run.transfer.phase = sample.phase.clone();
        run.transfer.bytes = sample.bytes;
        run.transfer.total = sample.total;
        run.transfer.permille = sample.permille;
        run.drawn = flashcard::render(i18n, &run.transfer, self.runner.rate);
        if self.shown {
            return Vec::new();
        }
        vec![Effect::HandTransfer(job_block(run))]
    }

    /// Which press a flashdone answers: the running flash, a refusal of the press just sent, or neither.
    pub fn finish_transfer(&mut self, i18n: &I18n, done: &wire::FlashDone) -> Vec<Effect> {
        let running = self.running();
        let transfer_id = self.runner.transfer.as_ref().map(|run| run.id).unwrap_or(0);
        match flashpane::done_for(
            running,
            transfer_id,
            self.runner.press_sent,
            self.runner.pending_id,
            done.id,
        ) {
            flashpane::DoneFor::Ignore => Vec::new(),
            flashpane::DoneFor::Refused => {
                self.runner.press_sent = false;
                // A probe refusal answers before any flashstarted, so the view asks the image row again.
                if done.phase == "probe" {
                    let path = self
                        .image
                        .as_ref()
                        .map(|image| image.path.clone())
                        .unwrap_or_default();
                    return vec![Effect::Send(wire::flash_image_request(&path))];
                }
                Vec::new()
            }
            flashpane::DoneFor::Finished => {
                self.runner.press_sent = false;
                let image = self
                    .image
                    .as_ref()
                    .map(|image| logic::leaf_of(&image.path))
                    .unwrap_or_default();
                let drive = self
                    .chosen_drive()
                    .map(|drive| flash::drive_name(drive, &self.drives))
                    .unwrap_or_default();
                let drive_bytes = self.chosen.as_ref().map(|drive| drive.bytes).unwrap_or(0);
                let err = if done.ok {
                    String::new()
                } else if done.err.is_empty() {
                    "error".into()
                } else {
                    done.err.clone()
                };
                let figures = match &self.runner.transfer {
                    Some(run) => flashpane::ending_context(&run.transfer, &err, done.at),
                    None => flashpane::Figures::default(),
                };
                let ctx = flashendings::Ctx {
                    image,
                    drive,
                    at: figures.at,
                    total: figures.total,
                    blank: done.blank,
                    tool: done.tool.clone(),
                    phase: done.phase.clone(),
                    drive_bytes,
                };
                let id = self
                    .runner
                    .transfer
                    .as_ref()
                    .map(|run| run.id)
                    .unwrap_or(transfer_id);
                self.runner.transfer = None;
                self.runner.rate = 0;
                if self.shown {
                    self.runner.ending = Some(flashendings::ending_for(i18n, &err, &ctx));
                    self.cursor = self.flash_row();
                    return Vec::new();
                }
                let status = flashendings::status_for(i18n, &err, &ctx);
                vec![
                    Effect::CloseJob(id),
                    Effect::Say {
                        text: status.text,
                        error: status.is_error,
                    },
                ]
            }
        }
    }

    pub fn finish_eject(&mut self, i18n: &I18n, eject: &wire::FlashEject) -> Vec<Effect> {
        let drive = self
            .chosen_drive()
            .map(|drive| flash::drive_name(drive, &self.drives))
            .unwrap_or_default();
        if eject.ok {
            self.ejected = true;
            return vec![Effect::Say {
                text: flashendings::eject_done_line(i18n, &drive),
                error: false,
            }];
        }
        if eject.err == "busy" {
            return vec![Effect::Say {
                text: flashendings::eject_busy_line(i18n, &drive),
                error: true,
            }];
        }
        vec![Effect::Say {
            text: flashendings::eject_failed_line(i18n, &drive),
            error: true,
        }]
    }

    /// A dead backend sends no flashdone: the press ends here, and a running flash ends as a write failure.
    pub fn on_failed(&mut self, i18n: &I18n) -> Vec<Effect> {
        self.runner.press_sent = false;
        let Some(run) = self.runner.transfer.as_ref() else {
            return Vec::new();
        };
        let done = wire::FlashDone {
            id: run.id,
            ok: false,
            phase: run.transfer.phase.clone(),
            at: run.transfer.bytes,
            err: "io".into(),
            blank: false,
            tool: String::new(),
        };
        self.finish_transfer(i18n, &done)
    }
}

pub(super) fn job_block(run: &Run) -> JobBlock {
    JobBlock {
        id: run.id,
        head: run.drawn.head_text.clone(),
        file: run.drawn.file_text.clone(),
        byte: run
            .drawn
            .byte_text
            .iter()
            .map(|part| (part.text.clone(), part.figure))
            .collect(),
        share: run.drawn.share,
        cancel: !run.drawn.no_cancel,
    }
}

#[cfg(test)]
#[test]
fn a_count_that_goes_back_restarts_the_rate_window() {
    let mut view = Flash::closed();
    view.note_rate(8_000, RATE_WINDOW_MS);
    assert_eq!(
        view.runner.rate, 4_000,
        "the write reads 8000 bytes over two seconds"
    );
    view.note_rate(1_000, RATE_WINDOW_MS + 500);
    assert_eq!(
        view.runner.rate_bytes, 1_000,
        "verify's first chunk restarts the window"
    );
    assert_eq!(
        view.runner.rate, 0,
        "the write's rate is not drawn while verify fills its first window"
    );
    view.note_rate(3_000, 2 * RATE_WINDOW_MS + 500);
    assert_eq!(
        view.runner.rate, 1_000,
        "verify reads its own 2000 bytes over two seconds, not the write's rate"
    );
}
