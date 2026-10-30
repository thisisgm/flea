//! The flash view as plain data: the strip title, the image row, one file row per drive, the Flash step.
use super::Flash;
use flea::i18n::I18n;
use flea::ui_logic_op2::{cards as logic, flash, flashendings};

impl Flash {
    /// The counts zone's own line while the host owns the pane.
    pub fn drives_line(&self, i18n: &I18n) -> String {
        flash::drives_line(i18n, self.drives.len())
    }

    pub fn ending(&self) -> Option<&flashendings::Ending> {
        self.runner.ending.as_ref()
    }

    pub(crate) fn cursor_index(&self) -> usize {
        self.cursor
    }

    pub(crate) fn drive_count(&self) -> usize {
        self.drives.len()
    }

    pub(crate) fn phase(&self) -> &str {
        self.runner
            .transfer
            .as_ref()
            .map_or("", |run| run.transfer.phase.as_str())
    }

    /// The view as the host draws it: the strip title, the image row, one file row per drive, the Flash step.
    pub fn view(&self, i18n: &I18n) -> View {
        let image = self.image.as_ref();
        let refused = image.is_some_and(|image| image.refused);
        let image_row = ImageRow {
            name: image
                .map(|image| logic::leaf_of(&image.path))
                .unwrap_or_else(|| i18n.tr("archive.flash.choose_image")),
            detail: image
                .map(|image| flash::image_detail(i18n, image))
                .unwrap_or_default(),
            size: image
                .map(|image| i18n.locale().format_size(image.bytes))
                .unwrap_or_default(),
            cursor: self.cursor == 0,
            refused,
            present: image.is_some(),
            hint: if refused {
                i18n.tr("archive.flash.not_image")
            } else {
                String::new()
            },
        };
        let drives = self
            .drives
            .iter()
            .enumerate()
            .map(|(at, drive)| {
                let selected = self.selected() == Some(at);
                DriveRow {
                    name: flash::drive_name(drive, &self.drives),
                    detail: flash::drive_detail(drive),
                    size: i18n.locale().format_size(drive.bytes),
                    cursor: self.cursor == at + 1,
                    selected,
                    dimmed: !flash::fits(drive),
                    hint: {
                        let key = flash::refused_hint_key(drive);
                        if key.is_empty() {
                            String::new()
                        } else {
                            i18n.tr(key)
                        }
                    },
                }
            })
            .collect();
        let words = flash::flash_words(
            i18n,
            image.is_some_and(|image| !image.refused),
            self.chosen_drive(),
            &self.drives,
        );
        let running = self.running();
        let (mark, line) = match (&self.runner.transfer, &self.runner.ending) {
            (Some(run), _) => (StepMark::Spinner, run.drawn.head_text.clone()),
            (None, Some(ending)) => (
                if ending.role == "error" {
                    StepMark::Alert
                } else if ending.mark == "check" {
                    StepMark::Check
                } else {
                    StepMark::Cross
                },
                ending.headline.clone(),
            ),
            (None, None) => (StepMark::Alert, words.erase.clone()),
        };
        let ending = self.runner.ending.as_ref();
        let (share, byte) = self
            .runner
            .transfer
            .as_ref()
            .map(|run| {
                (
                    run.drawn.share,
                    run.drawn
                        .byte_text
                        .iter()
                        .map(|part| (part.text.clone(), part.figure))
                        .collect(),
                )
            })
            .unwrap_or((0.0, Vec::new()));
        View {
            title: i18n.tr("archive.flash.title"),
            image: image_row,
            drives,
            drives_ready: self.drives_ready,
            step: StepRow {
                mark,
                line,
                action: words.action.clone(),
                live: self.live(),
                resting: !running && self.runner.ending.is_none() && !self.live(),
                running,
                hint: ending.map(|ending| ending.hint.clone()).unwrap_or_default(),
                cursor: self.cursor == self.flash_row(),
                ejected: self.ejected,
                end_label: match ending {
                    Some(ending) if ending.flash_back => words.action,
                    Some(_) => i18n.tr("archive.flash.eject"),
                    None => String::new(),
                },
                end_primary: ending.is_some_and(|ending| ending.flash_back),
                end_visible: ending.is_some_and(|ending| ending.eject || ending.flash_back)
                    && !self.ejected,
                action_hover: self.action_hover,
                action_pressed: self.action_pressed,
                cancel_live: self
                    .runner
                    .transfer
                    .as_ref()
                    .map(|run| !run.drawn.no_cancel)
                    .unwrap_or(true),
                share,
                byte,
            },
            status: self.drives_line(i18n),
            focused: self.shown,
        }
    }
}

/// The IMAGE step: the chosen image with its sizes or the picker row, and the refusal hint under a refused image.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageRow {
    pub name: String,
    pub detail: String,
    pub size: String,
    pub cursor: bool,
    pub refused: bool,
    pub present: bool,
    pub hint: String,
}

impl ImageRow {
    /// The picker row takes the folder mark; a named image the file mark.
    pub fn mark(&self) -> super::super::cards::Glyph {
        if self.present {
            super::super::cards::Glyph::File
        } else {
            super::super::cards::Glyph::FolderOpen
        }
    }
}

/// The DRIVES step: one file row per drive with its refusal hint, or the empty row while none is plugged in.
#[derive(Clone, Debug, PartialEq)]
pub struct DriveRow {
    pub name: String,
    pub detail: String,
    pub size: String,
    pub cursor: bool,
    pub selected: bool,
    pub dimmed: bool,
    pub hint: String,
}

/// The FLASH step's own mark: the alert, the running spiral, or the ending's mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepMark {
    Alert,
    Spinner,
    Check,
    Cross,
}

/// The FLASH step: the erase line and the action, then the running bar and the ending row.
#[derive(Clone, Debug, PartialEq)]
pub struct StepRow {
    pub mark: StepMark,
    pub line: String,
    pub action: String,
    pub live: bool,
    /// FlashStepRow.qml: at rest with no live Flash, the mark and the erase line dim.
    pub resting: bool,
    pub running: bool,
    pub hint: String,
    pub cursor: bool,
    pub ejected: bool,
    pub end_label: String,
    pub end_primary: bool,
    pub end_visible: bool,
    pub action_hover: bool,
    pub action_pressed: bool,
    /// Cancel dims in the final flush, when every byte is on the device; live otherwise while running.
    pub cancel_live: bool,
    /// The running bar and byte line: the share and the figured parts, empty at rest.
    pub share: f64,
    pub byte: Vec<(String, bool)>,
}

/// Which action the step row draws: Cancel while running, the ending button, Flash, or none after Eject.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepAction {
    Flash,
    Cancel,
    End,
    Hidden,
}

impl StepRow {
    pub fn step_action(&self) -> StepAction {
        if self.running {
            return StepAction::Cancel;
        }
        if self.end_visible {
            return StepAction::End;
        }
        if self.ejected {
            return StepAction::Hidden;
        }
        StepAction::Flash
    }

    pub fn show_bar(&self) -> bool {
        self.running
    }
}

/// The debug selector the draw code attaches to each flash state, pinned here beside the plan.
pub const SEL_IMAGE_ROW: &str = "op2-flash-image-row";
pub const SEL_DRIVE_ROW: &str = "op2-flash-drive";
pub const SEL_IMAGE_HINT: &str = "op2-flash-image-hint";
pub const SEL_EMPTY_MARK: &str = "op2-flash-empty-mark";
pub const SEL_CURSOR_EDGE: &str = "op2-flash-cursor-edge";
pub const SEL_CURSOR_EDGE_LIVE: &str = "op2-flash-cursor-edge-live";
pub const SEL_STEP_CANCEL: &str = "op2-flash-cancel";
pub const SEL_STEP_WASH_HOVER: &str = "op2-flash-wash-hover";
pub const SEL_STEP_WASH_PRESS: &str = "op2-flash-wash-press";
pub const SEL_STEP_BAR: &str = "op2-flash-bar";
pub const SEL_STEP_BYTE: &str = "op2-flash-byte";
pub const SEL_STEP_END: &str = "op2-flash-end";
pub const SEL_STEP_FLASH: &str = "op2-flash-action";
pub const SEL_STEP_FRAME: &str = "op2-flash-frame";
pub const SEL_FORMAT_LABEL: &str = "op2-compress-format-label";

/// The whole flash view as plain data.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub title: String,
    pub image: ImageRow,
    pub drives: Vec<DriveRow>,
    pub drives_ready: bool,
    pub step: StepRow,
    pub status: String,
    /// The window holds the keys while the flash is shown, so the cursor rung takes the accent edge.
    pub focused: bool,
}
