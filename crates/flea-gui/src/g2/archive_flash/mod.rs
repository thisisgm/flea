//! Archive and flash cards over core's ui_logic_archive_flash, drawn through the dialog shell seam.
pub mod cards;
pub mod compress;
mod compress_layout;
pub mod draw;
mod draw_controls;
pub mod draw_flash;
mod draw_flash_rows;
mod draw_press;
pub mod draw_step;
pub mod extract;
pub mod flash;
pub mod host;
pub mod keys;

#[cfg(test)]
mod card_tests;
#[cfg(test)]
mod draw_eye_tests;
#[cfg(test)]
mod draw_flash_tests;
#[cfg(test)]
mod draw_motion_tests;
#[cfg(test)]
mod draw_press_tests;
#[cfg(test)]
mod draw_radius_tests;
#[cfg(test)]
mod draw_step_tests;
#[cfg(test)]
mod draw_tests;
#[cfg(test)]
mod extract_tests;
#[cfg(test)]
mod fixcards_tests;
#[cfg(test)]
mod flash_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod label_tracking_tests;
#[cfg(test)]
mod scene_tests;
#[cfg(test)]
mod tests;

/// What a card asks its host to do; the host owns the backend, the panes and the status line.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// One request line for the backend.
    Send(String),
    /// The card is done and leaves the shell.
    Close,
    /// A status line; the sticky line while its send went out at once, an error sentence otherwise.
    Say { text: String, error: bool },
    /// The running flash joins the pane's transfer the way a copy's does, while the view is closed.
    HandTransfer(cards::JobBlock),
    /// The handed run ended while hidden: the pane's Jobs card closes the block by id, as JobsSink does.
    CloseJob(u64),
    /// Reopening onto a running flash takes its card back from the pane; a copy's card stays.
    TakeBack,
    /// The image row asks the host to run the image chooser over the downloads folder.
    PickImage,
}

/// One card at a time: a second request is answered with a sentence and never dropped silently.
pub fn busy_sentence(i18n: &flea::i18n::I18n) -> String {
    i18n.tr("archive.busy")
}

/// The host's guard before opening either password card, as ArchiveCards.cardBusy answers.
pub fn card_busy(
    compress_opened: bool,
    extract_opened: bool,
    i18n: &flea::i18n::I18n,
) -> Option<Effect> {
    if compress_opened || extract_opened {
        Some(Effect::Say {
            text: busy_sentence(i18n),
            error: true,
        })
    } else {
        None
    }
}
