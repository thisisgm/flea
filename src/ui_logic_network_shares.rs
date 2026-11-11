//! Network form, identity and bridge rules shared by front ends, with no I/O.
pub mod bridge;
pub mod browser;
pub mod cloud;
pub mod form;
pub mod heading;
mod kernel_mounts;
pub mod places;
pub mod rail;
pub mod smb;

pub use crate::backend::networkaddress;
pub use crate::ui_logic_device_rail::mounts;
pub use crate::ui_logic_device_rail::protocols;

pub const DROPBOX_INSTALL: [&str; 5] = [
    "omarchy-launch-terminal",
    "omarchy",
    "install",
    "service",
    "dropbox",
];

pub fn local_path(output: &str) -> &str {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix("local path: "))
        .unwrap_or("")
}

pub fn credentialed(uri: &str) -> bool {
    protocols::authority(uri).contains('@')
}

pub fn keyless(uri: &str) -> bool {
    protocols::scheme_of(uri) == "sftp"
}

pub fn bare_root(uri: &str) -> bool {
    uri.split_once("://")
        .is_some_and(|(_, rest)| !rest.trim_end_matches('/').contains('/'))
}

#[cfg(test)]
mod fix1_tests;
#[cfg(test)]
mod gh374_tests;
#[cfg(test)]
mod oracle_tests;
#[cfg(test)]
mod tests;
