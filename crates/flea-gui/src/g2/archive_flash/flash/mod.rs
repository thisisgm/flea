//! Flash image swaps the listing like Trash does; Esc, Back and Backspace leave, never cancel a flash.
use super::Effect;
use flea::ui_logic_archive_flash::{flash, flashcard, flashendings, flashpane, wire};
use std::time::{Duration, Instant};

/// The keyboard's seat: the image, a drive, the Flash step, its Cancel, its ending, or the pane itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seat {
    Image,
    Drive,
    Flash,
    Cancel,
    End,
    Pane,
}

/// The running flash with its rendered lines beside its fields.
#[derive(Clone, Debug)]
struct Run {
    id: u64,
    transfer: flashcard::Transfer,
    drawn: flashcard::Drawn,
}

/// The flash's own run, answered by the backend; the view owns the cursor and the rows.
#[derive(Clone, Debug, Default)]
struct Runner {
    transfer: Option<Run>,
    ending: Option<flashendings::Ending>,
    rate: u64,
    rate_at: u64,
    rate_bytes: u64,
    press_sent: bool,
    pending_id: u64,
}

/// The rate is the bytes since the last sample over at least this many seconds, so one burst never reads as speed.
const RATE_WINDOW_MS: u64 = 2000;
/// The rate is bytes per second from millisecond stamps.
const MS_PER_SECOND: u64 = 1000;
/// Buttons.PRESS_MS: the pressed chrome slides in and out over 150 ms.
const PRESS_MS: u64 = 150;

/// The pressed chrome's slide from rest (0) to the whole-pixel inset (1), OutCubic from where the last change found it, like rail's Slide.
#[derive(Clone, Copy, Debug)]
pub struct PressAnim {
    pub from: f32,
    pub to: f32,
    pub start: Instant,
}

impl PressAnim {
    /// A chrome that never moves: rest, or fully pressed under reduced motion.
    pub fn settled(shown: bool) -> Self {
        let to = if shown { 1. } else { 0. };
        Self {
            from: to,
            to,
            start: Instant::now(),
        }
    }

    /// OutCubic over PRESS_MS from where the last press change found it.
    pub fn at(&self, now: Instant) -> f32 {
        let total = Duration::from_millis(PRESS_MS).as_secs_f32();
        let t = (now.saturating_duration_since(self.start).as_secs_f32() / total).min(1.);
        let eased = 1. - (1. - t).powi(3);
        self.from + (self.to - self.from) * eased
    }
}

mod run;
use run::job_block;
mod view;

pub use view::{
    DriveRow, ImageRow, StepAction, StepMark, StepRow, View, SEL_CURSOR_EDGE, SEL_CURSOR_EDGE_LIVE,
    SEL_DRIVE_ROW, SEL_EMPTY_MARK, SEL_FORMAT_LABEL, SEL_IMAGE_HINT, SEL_IMAGE_ROW, SEL_STEP_BAR,
    SEL_STEP_BYTE, SEL_STEP_CANCEL, SEL_STEP_END, SEL_STEP_FLASH, SEL_STEP_FRAME,
    SEL_STEP_WASH_HOVER, SEL_STEP_WASH_PRESS,
};

#[derive(Clone, Debug)]
pub struct Flash {
    image: Option<flash::Image>,
    pending_drive: String,
    drives: Vec<flash::Drive>,
    drives_ready: bool,
    chosen: Option<flash::Drive>,
    held: bool,
    cursor: usize,
    request_id: u64,
    ejected: bool,
    shown: bool,
    /// The view has opened once, as FlashHost builds its FlashView on first use and keeps it.
    built: bool,
    action_hover: bool,
    action_pressed: bool,
    press: PressAnim,
    runner: Runner,
}

impl Flash {
    pub fn closed() -> Self {
        Self {
            image: None,
            pending_drive: String::new(),
            drives: Vec::new(),
            drives_ready: false,
            chosen: None,
            held: false,
            cursor: 0,
            request_id: 0,
            ejected: false,
            shown: false,
            built: false,
            action_hover: false,
            action_pressed: false,
            press: PressAnim::settled(false),
            runner: Runner::default(),
        }
    }

    /// The pointer's hover and press over the step action, drawn locally like pl3's strip button.
    pub fn set_action_hover(&mut self, hover: bool, now: Instant) {
        self.action_hover = hover;
        self.retarget_press(now);
    }

    pub fn set_action_pressed(&mut self, pressed: bool, now: Instant) {
        self.action_pressed = pressed;
        self.retarget_press(now);
    }

    fn retarget_press(&mut self, now: Instant) {
        let shown = self.action_pressed && self.action_hover && self.step_available();
        let to = if shown { 1. } else { 0. };
        if self.press.to != to {
            self.press = PressAnim {
                from: self.press.at(now),
                to,
                start: now,
            };
        }
    }

    pub fn press_anim(&mut self, now: Instant) -> &PressAnim {
        self.retarget_press(now);
        &self.press
    }

    /// The image the view names, so the host can match a flashimage reply to it.
    pub fn image_path(&self) -> Option<&str> {
        self.image.as_ref().map(|image| image.path.as_str())
    }

    pub fn shown(&self) -> bool {
        self.shown
    }

    pub fn built(&self) -> bool {
        self.built
    }

    pub fn running(&self) -> bool {
        self.runner.transfer.is_some()
    }

    pub fn live(&self) -> bool {
        let image_ok = self.image.as_ref().is_some_and(|image| !image.refused);
        flash::flash_live(image_ok, self.chosen.as_ref())
    }

    pub fn selected(&self) -> Option<usize> {
        self.chosen
            .as_ref()
            .and_then(|chosen| flashpane::index_of(&self.drives, chosen))
    }

    fn flash_row(&self) -> usize {
        flash::flash_pos(self.drives.len())
    }

    fn chosen_drive(&self) -> Option<&flash::Drive> {
        self.selected().and_then(|at| self.drives.get(at))
    }

    /// Reopening onto a running flash shows it, never restarts it, and takes its card back from the pane.
    pub fn open(&mut self) -> Vec<Effect> {
        self.action_hover = false;
        self.action_pressed = false;
        self.press = PressAnim::settled(false);
        self.shown = true;
        self.built = true;
        self.ejected = false;
        if self.running() {
            return vec![Effect::TakeBack];
        }
        // A sent press is a flash in all but its flashstarted, so a reopen keeps its image, drive and runner.
        if self.runner.press_sent {
            return Vec::new();
        }
        self.runner = Runner::default();
        self.drives.clear();
        self.drives_ready = false;
        Vec::new()
    }

    /// After Back the running flash joins the pane's transfer the way a copy's does.
    pub fn close(&mut self) -> Vec<Effect> {
        self.action_hover = false;
        self.action_pressed = false;
        self.press = PressAnim::settled(false);
        if !self.shown {
            return Vec::new();
        }
        self.shown = false;
        match &self.runner.transfer {
            Some(run) => vec![Effect::HandTransfer(job_block(run))],
            None => vec![Effect::Close],
        }
    }

    pub fn press_flash(&mut self) -> Vec<Effect> {
        if self.runner.press_sent || !self.live() || self.running() {
            return Vec::new();
        }
        let Some(drive) = self.chosen_drive().cloned() else {
            return Vec::new();
        };
        let Some(image) = self.image.as_ref() else {
            return Vec::new();
        };
        self.request_id += 1;
        self.runner.press_sent = true;
        self.runner.pending_id = self.request_id;
        vec![Effect::Send(wire::flash_request(
            self.request_id,
            &image.path,
            &drive.dev,
            drive.seq,
            &drive.serial,
            drive.bytes,
        ))]
    }

    pub fn press_cancel(&mut self) -> Vec<Effect> {
        match &self.runner.transfer {
            Some(run) => vec![Effect::Send(wire::flash_cancel_request(run.id))],
            None => Vec::new(),
        }
    }

    pub fn eject_chosen(&mut self) -> Vec<Effect> {
        if self.running() || self.ejected {
            return Vec::new();
        }
        match self.chosen_drive() {
            Some(drive) => vec![Effect::Send(wire::flash_eject_request(
                &drive.dev, drive.seq,
            ))],
            None => Vec::new(),
        }
    }

    /// The ending row's own action: Eject where one is offered, Flash again under a failure.
    pub fn press_end(&mut self) -> Vec<Effect> {
        match &self.runner.ending {
            Some(ending) if ending.flash_back => self.press_flash(),
            Some(_) => self.eject_chosen(),
            None => Vec::new(),
        }
    }

    fn cursor_drive(&self) -> Option<usize> {
        if self.cursor > 0 && self.cursor <= self.drives.len() {
            Some(self.cursor - 1)
        } else {
            None
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        // While a flash runs the cursor stays between the drive being flashed and Cancel.
        if self.running() {
            if self.cursor == self.flash_row() {
                self.cursor = self.selected().map_or(0, |at| at + 1);
            } else {
                self.cursor = self.flash_row();
            }
            return;
        }
        // FlashView.qml moveCursor: the step row is a stop even while Flash is greyed; press_step refuses it there.
        self.cursor = flash::step(self.cursor, delta, &self.drives, true);
        self.chosen = flashpane::choose_kept(self.cursor, &self.drives, self.chosen.clone());
    }

    /// The image row's pointer entry: the picker opens only while nothing runs.
    pub fn choose_image(&mut self) -> Vec<Effect> {
        if self.running() {
            return Vec::new();
        }
        self.cursor = 0;
        vec![Effect::PickImage]
    }

    /// A drive row's pointer entry: a refused drive is never chosen, and nothing is chosen while a flash runs.
    pub fn choose_drive(&mut self, index: usize) -> Vec<Effect> {
        if self.running() {
            return Vec::new();
        }
        let Some(drive) = self.drives.get(index).cloned() else {
            return Vec::new();
        };
        if !flash::fits(&drive) {
            return Vec::new();
        }
        self.chosen = Some(drive);
        self.cursor = self.flash_row();
        Vec::new()
    }

    /// Cancel is live everywhere except the final flush, where Transfer.js cancelEnabled is false.
    pub fn cancel_live(&self) -> bool {
        self.runner
            .transfer
            .as_ref()
            .map(|run| !run.drawn.no_cancel)
            .unwrap_or(true)
    }

    /// The drawn step action's availability, mirroring DialogButton.qml: only an available button activates.
    pub fn step_available(&self) -> bool {
        if self.running() {
            return self.cancel_live();
        }
        match &self.runner.ending {
            Some(ending) if (ending.flash_back || ending.eject) && !self.ejected => {
                !ending.flash_back || self.live()
            }
            Some(_) => false,
            None => {
                if self.ejected {
                    false
                } else {
                    self.live()
                }
            }
        }
    }

    /// The step action's own press: Cancel while running, the ending button, Flash where live.
    pub fn press_step(&mut self) -> Vec<Effect> {
        if !self.step_available() {
            return Vec::new();
        }
        if self.running() {
            return self.press_cancel();
        }
        if self.runner.ending.is_some() {
            return self.press_end();
        }
        self.press_flash()
    }

    pub fn context_press(&mut self) -> Vec<Effect> {
        if self.cursor == self.flash_row() {
            return self.press_step();
        }
        if self.running() {
            return Vec::new();
        }
        if self.runner.ending.is_some() {
            if self.cursor == 0 {
                return self.choose_image();
            }
            if let Some(at) = self.cursor_drive() {
                return self.choose_drive(at);
            }
            return Vec::new();
        }
        if self.cursor == 0 {
            return self.choose_image();
        }
        if let Some(at) = self.cursor_drive() {
            return self.choose_drive(at);
        }
        Vec::new()
    }
}

mod input;
