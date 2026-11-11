//! Optional test service factories. With no override, Shell::new uses its normal services.
use super::*;

pub(crate) struct Services {
    pub backend: fn(&crate::navigation::Listing, &mut Context<Shell>) -> std::io::Result<Backend>,
    pub monitor: Option<fn() -> crate::g2::trash_browser::monitor::Started>,
}
impl gpui::Global for Services {}

/// Keeps a test window off the real flea-settings thread, whose wakes the test scheduler rejects.
pub(crate) struct NoSettingsWorker;
impl gpui::Global for NoSettingsWorker {}

pub(crate) fn isolated_services(cx: &mut gpui::App) {
    cx.set_global(NoSettingsWorker);
    cx.set_global(Services {
        backend: |_, cx| {
            let (backend, requests) = Backend::test_channel();
            cx.on_release(move |_, _| drop(requests)).detach();
            Ok(backend)
        },
        monitor: None,
    });
}

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
    pane: &gpui::Entity<crate::g2::trash_browser::pane::TrashPane>,
    cx: &mut Context<Shell>,
) {
    if let Some(start) = cx.try_global::<Services>().map(|services| services.monitor) {
        pane.update(cx, |pane, _| pane.monitor_start = start);
    }
}
