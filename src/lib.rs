//! Shared, dependency-free protocol primitives for Flea frontends.

pub mod i18n;
pub mod ipc;
pub mod json;
pub mod jsondoc;
pub mod keys;
mod jsonstring;
pub mod oflags;
pub mod protocol;

// The catalog and locale API shares the backend's regular-file and wall-clock primitives.
pub mod backend {
    #[cfg(test)]
    pub mod fifotest;
    pub mod regfile;
    #[cfg(test)]
    pub mod testdir;
    pub mod trashdate;
}
