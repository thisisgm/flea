//! Optional test service factories. With no override, Shell::new uses its normal services.
use super::*;

pub(crate) struct Services {
    pub backend: fn(&crate::navigation::Listing, &mut Context<Shell>) -> std::io::Result<Backend>,
    pub monitor: Option<fn() -> crate::g2::pl3::monitor::Started>,
}
impl gpui::Global for Services {}

/// Keeps a test window off the real flea-settings thread, whose wakes the test scheduler rejects.
pub(crate) struct NoSettingsWorker;
impl gpui::Global for NoSettingsWorker {}

pub(super) fn start_backend(
    listing: &crate::navigation::Listing,
    cx: &mut Context<Shell>,
) -> std::io::Result<Backend> {
    match cx.try_global::<Services>().map(|services| services.backend) {
        Some(start) => start(listing, cx),
        None => Backend::start_listing(listing),
    }
}

pub(super) fn configure_monitor(
    pane: &gpui::Entity<crate::g2::pl3::pane::TrashPane>,
    cx: &mut Context<Shell>,
) {
    if let Some(start) = cx.try_global::<Services>().map(|services| services.monitor) {
        pane.update(cx, |pane, _| pane.monitor_start = start);
    }
}
