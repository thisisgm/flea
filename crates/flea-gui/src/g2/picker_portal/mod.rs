//! Picker portal and lifetime: the request, its one answer, the reply file and both read-only children.
mod answer;
mod answers;
mod body;
pub mod browse;
mod browse_reply;
pub mod chrome;
mod chrome_view;
mod drawn;
pub mod filename;
mod filters;
mod find;
mod find_input;
mod focus;
mod folder;
mod footer;
mod geometry;
mod header;
mod ime;
pub mod ipc;
mod jump;
mod keys;
pub mod labels;
pub mod launch;
pub mod navigation;
mod network;
pub mod picker;
mod pointer;
pub mod process;
pub mod rail;
mod rail_view;
pub mod reply;
mod restore;
mod route;
pub mod selection;
mod settings;
mod size;
mod text;
mod view;

pub use picker::{Children, ListingFacts, Message, Picker};

#[cfg(test)]
mod i50_dispatch_tests;
#[cfg(test)]
mod i50_ime_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod c2_tests;
#[cfg(test)]
mod cohesion_tests;
#[cfg(test)]
mod corner_tests;
#[cfg(test)]
mod ey200_chrome_tests;
#[cfg(test)]
mod ey263_field_tests;
#[cfg(test)]
mod ey302_footer_tests;
#[cfg(test)]
mod ey313_tests;
#[cfg(test)]
mod ey372_single_click_tests;
#[cfg(test)]
mod ey377_list_wheel_tests;
#[cfg(test)]
mod face_tests;
#[cfg(test)]
mod filter_fade_tests;
#[cfg(test)]
mod fix_picker_2b_tests;
#[cfg(test)]
mod fixchrome_tests;
#[cfg(test)]
mod fixlogic_ey485_tests;
#[cfg(test)]
mod header_tests;
#[cfg(test)]
mod i50_fit_tests;
#[cfg(test)]
mod new_folder_tests;
#[cfg(test)]
mod parity_tests;
#[cfg(test)]
mod pointer_row_tests;
#[cfg(test)]
mod pointer_state_tests;
#[cfg(test)]
mod pointer_tests;
#[cfg(test)]
mod rail2_tests;
#[cfg(test)]
mod rail_press_tests;
#[cfg(test)]
mod rail_tests;
#[cfg(test)]
mod save_click_space_tests;
#[cfg(test)]
mod save_enabled_tests;
#[cfg(test)]
mod save_field_pointer_tests;
#[cfg(test)]
mod word_click_tests;
#[cfg(test)]
mod word_navigation_tests;

#[cfg(test)]
mod board_parity_tests;
#[cfg(test)]
mod ey_chooser_tests;
#[cfg(test)]
mod fix_picker_2a_tests;
#[cfg(test)]
mod frame_gate_tests;
#[cfg(test)]
mod gm338_hidden_tests;
#[cfg(test)]
mod recent_parity_tests;
#[cfg(test)]
mod save_layout_tests;
#[cfg(test)]
mod settings_parity_tests;
