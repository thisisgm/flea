//! Shared, dependency-free protocol primitives for Flea frontends.
extern crate self as flea;

pub mod colours;
pub mod error;
pub mod i18n;
pub mod ipc;
pub mod json;
pub mod jsondoc;
pub mod favourites;
pub mod ui_logic_pv1;
pub mod uischema;
pub mod uistate;
pub mod uistore;
pub mod userfile;
mod uimigrate;
mod uiplugins;
pub mod keys;
mod jsonstring;
pub mod oflags;
pub mod protocol;
pub mod preview_model;
pub mod theme;
pub mod ui_logic_fb1;
pub mod ui_logic_dg1;
pub mod ui_logic_mn1;
pub mod ui_logic_mn2;
pub mod ui_logic_op1;
pub mod ui_logic_op3;
pub mod ui_logic_op4;
pub mod ui_logic_pk1;
pub mod ui_logic_pk2;
pub mod ui_logic_pl1;
pub mod ui_logic_pl2;
pub mod ui_logic_pl3;
pub mod ui_logic_pl4;
pub mod ui_logic_pv2;
pub mod ui_logic_pv3;
pub mod ui_logic_pv4;
pub mod ui_logic_sd1;
pub mod ui_logic_sd2;
pub mod ui_logic_sr1;
pub mod ui_logic_sr2;
pub mod ui_logic_st1;
pub mod ui_logic_st2;

// The catalog and locale API shares the backend's regular-file and wall-clock primitives.
pub mod backend {
    pub mod netfs;
    pub mod networkaddress;
    #[cfg(test)]
    pub mod nearbyrows;
    #[cfg(test)]
    pub mod fifotest;
    pub mod regfile;
    #[cfg(test)]
    pub mod testdir;
    pub mod trashdate;
}
