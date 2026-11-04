//! G2 surface families; each unit keeps its GUI half under its own module.
pub mod dg1;
pub mod dg2;
#[cfg(test)]
mod error_ink_tests;
pub mod fb1;
pub mod mn1;
pub mod mn2;
pub mod op1;
pub mod op2;
pub mod op3;
pub mod op4;
pub mod pk1;
pub mod pk2;
pub mod pl1;
pub mod pl2;
pub mod pl3;
pub mod pl4;
pub(crate) mod plugins;
pub mod pv1;
pub mod pv2;
pub mod pv3;
pub mod pv4;
pub mod sd1;
pub mod sd2;
pub mod sr1;
pub mod sr2;
pub mod st1;
pub mod st2;
