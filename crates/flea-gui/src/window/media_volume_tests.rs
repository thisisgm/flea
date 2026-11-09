//! EY-399: the saved preview.volume is the level every player opens at, and a kept level is saved back.
use crate::{backend::Backend, g2::media_preview::sound::MediaSound};
use flea::jsondoc::Json;
use gpui::TestAppContext;
use std::sync::Arc;

#[gpui::test]
fn ey399_the_saved_volume_seeds_the_players_and_a_kept_level_is_saved(cx: &mut TestAppContext) {
    let handle = cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        });
        cx.set_global(crate::state::constructor_fixture::NoSettingsWorker);
        let settings = super::startup::Settings::from_json(
            flea::jsondoc::parse(r#"{"view":"list","preview":{"volume":30}}"#).unwrap(),
        );
        let launch = super::startup::Launch::resolve(
            settings,
            super::startup::Arguments {
                path: Some("/files".into()),
                select: None,
            },
            "/home/test",
        );
        let locale = super::locale::Snapshot {
            revision: 1,
            catalog: Arc::new(crate::test_catalogs::english()),
        };
        let (_, events) = async_channel::unbounded();
        super::open(launch, locale, events, cx).expect("the production startup opens")
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| MediaSound::get(cx).volume()),
        30,
        "v0.3.12 ViewState.mediaVolume: players open at the saved level"
    );
    cx.update(|cx| MediaSound::kept(Some(55), cx));
    cx.run_until_parked();
    let saved = handle
        .update(cx, |root, _, cx| {
            root.shell
                .read(cx)
                .launch
                .settings
                .data
                .get("preview")
                .and_then(|preview| preview.get("volume").and_then(Json::as_f64))
        })
        .unwrap();
    assert_eq!(
        saved,
        Some(55.),
        "ViewState.setMediaVolume writes the kept level"
    );
}
