//! Confirmation, transfers and Jobs: one host, one dialog shell, the cards and the Jobs strip.
mod cards;
mod copy_to;
mod dialog;
mod feedback;
mod geometry;
pub mod glyph;
pub mod host;
pub mod ipc;
mod line;
pub mod model;
mod new_file;
pub mod palette;
pub mod press;
mod rclone;
pub mod view;

#[cfg(test)]
mod draw_tests;
#[cfg(test)]
mod fix10_tests;
#[cfg(test)]
mod fix11_overflow_tests;
#[cfg(test)]
mod fix11_paint_tests;
#[cfg(test)]
mod fix1_tests;
#[cfg(test)]
mod fix2_tests;
#[cfg(test)]
mod fix3_tests;
#[cfg(test)]
mod fix4_tests;
#[cfg(test)]
mod fix5_tests;
#[cfg(test)]
mod fix6_tests;
#[cfg(test)]
mod fix9_tests;
#[cfg(test)]
mod font_tests;
#[cfg(test)]
mod held_head_tests;
#[cfg(test)]
mod join_tests;
#[cfg(test)]
mod line_tests;
#[cfg(test)]
mod queued_paste_tests;
#[cfg(test)]
mod tests;

mod collision;

pub(crate) mod editor;

pub(crate) mod field;

mod focus;

mod input;

pub mod join;

pub(crate) mod measured;

mod progress;
mod transfer_status;
#[cfg(test)]
mod uish_def1_tests;

#[cfg(test)]
mod atomic_dialog_tests;

#[cfg(test)]
mod atomic_tests;

#[cfg(test)]
mod draw_fixture;

#[cfg(test)]
mod spin_tests;

#[cfg(test)]
mod field_fix4_tests;

#[cfg(test)]
mod field_input_tests;

#[cfg(test)]
mod field_tests;

#[cfg(test)]
mod focus_async_tests;

#[cfg(test)]
mod i50_fit_tests;

#[cfg(test)]
mod i50_new_file_tests;

#[cfg(test)]
mod publish_tests;

#[cfg(test)]
#[rustfmt::skip]
mod review_round2_tests;

#[cfg(test)]
mod routing_tests;

#[cfg(test)]
mod u13_draw_tests;

#[cfg(test)]
mod u13_join_tests;

pub(crate) mod dialog_editor;

pub(crate) mod field_view;
