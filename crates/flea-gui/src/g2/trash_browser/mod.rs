//! pl3, the Trash browser: the core session in flea::ui_logic_trash_browser drawn by custom Elements behind narrow seams.
pub mod board;
mod controls;
#[cfg(test)]
mod edge_tests;
mod elide;
#[cfg(test)]
mod fix_tests;
#[cfg(test)]
mod flow_tests;
pub(crate) mod glyph;
#[cfg(test)]
mod hover_tests;
pub mod ink;
pub mod join;
mod join_atomic;
mod join_cards;
mod join_input;
mod join_ipc;
mod join_menu;
#[cfg(test)]
mod join_r4_tests;
#[cfg(test)]
mod join_r5_tests;
#[cfg(test)]
mod join_readiness_tests;
#[cfg(test)]
mod join_review_tests;
mod join_sheet;
mod join_status;
mod join_sweep;
#[cfg(test)]
mod join_tests;
pub mod list;
mod list_ground;
#[cfg(test)]
mod list_readiness_tests;
pub mod monitor;
#[cfg(test)]
mod paint_tests;
pub mod pane;
pub mod presentation;
pub mod readers;
mod readers_controls;
#[cfg(test)]
mod s3_tests;
mod scrollbar;
pub mod seams;
#[cfg(test)]
mod side_button_trash_tests;
mod strip;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod trash_tab_tests;

#[cfg(test)]
mod join_atomic_tests;

#[cfg(test)]
mod gcomp_join_tests;

#[cfg(test)]
mod cover_focus_tests;
#[cfg(test)]
mod parity_input_tests;
#[cfg(test)]
mod parity_sweep_tests;
#[cfg(test)]
mod relist_tests;
#[cfg(test)]
mod status_line_tests;
#[cfg(test)]
mod tab_reach_tests;
#[cfg(test)]
mod trashview_tests;
#[cfg(test)]
mod uish_def1_tests;
#[cfg(test)]
mod undo_tests;
#[cfg(test)]
mod window_tests;
