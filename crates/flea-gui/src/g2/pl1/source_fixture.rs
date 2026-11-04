//! Only external device services are injected; Shell::new still mounts its real rail, and no test reads /etc/hostname.
use super::*;

pub(crate) struct Services {
    pub runner: Rc<dyn Runner>,
    pub hostname: String,
}

impl gpui::Global for Services {}

pub(crate) fn services(cx: &gpui::App) -> (Rc<dyn Runner>, Option<String>) {
    cx.try_global::<Services>()
        .map(|services| (services.runner.clone(), Some(services.hostname.clone())))
        .unwrap_or_else(|| (Rc::new(EmptyRunner), Some("testhost".into())))
}

impl Source {
    pub(crate) fn fixture_mount_listing(&mut self) -> &mut Listing {
        &mut self.listing
    }
}
