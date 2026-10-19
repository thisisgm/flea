//! Shared, dependency-free protocol primitives for Flea frontends.

pub mod colours;
pub mod i18n;
pub mod ipc;
pub mod json;
pub mod jsondoc;
pub mod keys;
mod jsonstring;
pub mod oflags;
pub mod protocol;
pub mod ui_logic_mn1;
pub mod ui_logic_pl3;
pub mod ui_logic_sd2;
pub mod ui_logic_sr1;

// The catalog and locale API shares the backend's regular-file and wall-clock primitives.
pub mod backend {
    #[cfg(test)]
    pub mod fifotest;
    pub mod regfile;
    #[cfg(test)]
    pub mod testdir;
    pub mod trashdate;
}
