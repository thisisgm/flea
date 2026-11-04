//! Agent, custom actions and plugin rows in the GPUI window: mn2 draws no Element of its own (rows reach the menu primitive as `mn1` entries, action progress rides the shared Jobs card, previews open only through preview rendering), and this module owns the menu's side of the lifecycle, all driven by the backend lines it is given.
pub mod join;
pub mod launch;
pub mod model;

#[cfg(test)]
mod join_tests;
#[cfg(test)]
mod launch_tests;
#[cfg(test)]
mod route_tests;
#[cfg(test)]
mod tests;
