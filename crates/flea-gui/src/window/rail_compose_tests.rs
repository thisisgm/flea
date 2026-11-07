//! Production-constructor controls. These never replace the rail's sections with fixture rows.
use super::*;
use crate::window::{
    locale::Snapshot,
    startup::{Arguments, Launch, Settings, View},
};
use flea::ui_logic_pl4::rail::Kind;
use gpui::{TestAppContext, VisualTestContext};

fn open<'a>(
    hidden: bool,
    view: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let document = flea::jsondoc::parse(&format!(
        r#"{{"view":"{view}","density":"compact","columnsLimit":3,"places":{{"rail":"{}","showHome":true,"showTrash":true}}}}"#,
        if hidden { "hidden" } else { "shown" },
    )).unwrap();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let launch = Launch::resolve(Settings::from_json(document), Arguments::default(), &home);
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let (_sender, events) = async_channel::unbounded();
    // Inject only external process factories before the real constructor starts them.
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = crate::backend::Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        })
    });
    let result = cx.add_window_view(|window, cx| Shell::new(launch, locale, events, window, cx));
    result.1.run_until_parked();
    result.0.read_with(result.1, |shell, cx| {
        assert!(shell
            .panes
            .iter()
            .flatten()
            .all(|host| host.backend.is_some() && !host.stopped));
        let trash = shell
            .trash
            .as_ref()
            .expect("the real constructor mounted Trash");
        assert!(trash.pane.read(cx).monitor_start.is_none());
    });
    result
}

fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn production_constructor_mounts_real_saved_places_in_all_three_views(cx: &mut TestAppContext) {
    for (name, expected) in [
        ("list", View::List),
        ("columns", View::Columns),
        ("grid", View::Grid),
    ] {
        let (shell, cx) = open(false, name, cx);
        for _ in 0..3 {
            frame(cx);
        }
        shell.read_with(cx, |shell, cx| {
            assert_eq!(shell.view, expected);
            let join = shell
                .rail
                .as_ref()
                .expect("Shell::new must mount the rail producer");
            let mounted = shell
                .slots
                .rail
                .clone()
                .expect("the production rail slot must contain its host")
                .downcast::<Host>()
                .expect("the slot must contain the real lazy Host");
            assert_eq!(mounted, join.host);
            assert_eq!(join.host.read(cx).constructed, 1);
            let producer = join.places.read(cx);
            assert_eq!(producer.directory_reads, 1);
            let entries = producer.entries();
            assert!(entries.iter().any(|entry| entry.kind == Kind::Home));
            assert!(entries.iter().any(|entry| entry.kind == Kind::Trash));
            let laid = producer.laid.borrow();
            assert!(laid.painted);
            assert!(
                laid.rows.len() >= 2,
                "the reserved rail must actually paint its saved rows"
            );
            assert_eq!(laid.width, join.host.read(cx).width());
            assert!(laid
                .rows
                .iter()
                .all(|row| row.rect.width > 0. && row.rect.height > 0.));
        });
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
fn production_hidden_rail_constructs_no_rows_and_starts_no_directory_read(cx: &mut TestAppContext) {
    let (shell, cx) = open(true, "columns", cx);
    for _ in 0..3 {
        frame(cx);
    }
    shell.read_with(cx, |shell, cx| {
        let join = shell
            .rail
            .as_ref()
            .expect("hidden mode still has its real lazy host");
        assert_eq!(join.host.read(cx).constructed, 0);
        let producer = join.places.read(cx);
        assert!(!producer.polling);
        assert_eq!(producer.directory_reads, 0);
        assert!(!producer.laid.borrow().painted);
        assert!(!producer.model.borrow().focused);
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn accepted_settings_and_typeface_reach_the_mounted_producer(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, "list", cx);
    for _ in 0..3 {
        frame(cx);
    }
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let fonts = crate::presentation::Fonts::from_sources(
                "[font]\nbase-size=18",
                "",
                "monospace".into(),
            );
            cx.set_global(fonts);
            let mut data = shell.launch.settings.data.clone();
            crate::g2::st1::seams::merge(
                &mut data,
                &flea::jsondoc::parse(r#"{"places":{"showHome":false,"showTrash":true}}"#).unwrap(),
            );
            shell.apply_settings(data, window, cx);
        })
    });
    frame(cx);
    shell.read_with(cx, |shell, cx| {
        let producer = shell.rail.as_ref().unwrap().places.read(cx);
        assert!(!producer
            .entries()
            .iter()
            .any(|entry| entry.kind == Kind::Home));
        assert!(producer
            .entries()
            .iter()
            .any(|entry| entry.kind == Kind::Trash));
        assert_eq!(producer.typeface, shell.active_typeface());
        assert_eq!(f32::from(producer.typeface.base), 18.);
        // The approved rail anchors interpolate body14's 29 px to body20's 41 px.
        assert_eq!(producer.laid.borrow().rows[0].rect.height, 37.);
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[path = "rail_shortcut_tests.rs"]
mod shortcuts;

#[path = "fixcards_flash_tests.rs"]
mod fixcards_flash;

#[path = "rail_language_tests.rs"]
mod rail_language;

#[path = "rail_language_every_tests.rs"]
mod rail_language_every;
