//! Every mounted application surface must shape with the presentation family's real face.
mod dialogs;
mod isolation;
mod picker;
mod readers;
mod receipt;
mod service;
mod settings;
mod surfaces;
mod test_font;

use crate::{state::Shell, window::root::Root};
use gpui::{AppContext, Context, TestAppContext, TestDispatcher, Window, WindowHandle};
use std::sync::Arc;

const FAMILY: &str = "Font Gate Mono";
const FALLBACK: &str = "JetBrainsMono Nerd Font";
const SECOND: &str = "Font Gate Second";
type Handle = WindowHandle<Root>;

fn context() -> (TestAppContext, Arc<receipt::Recorder>) {
    let recorder = receipt::Recorder::new();
    let cx = TestAppContext::build_with_text_system(TestDispatcher::new(0), None, recorder.clone());
    let (_service, snapshot) = crate::presentation::Service::start();
    assert_eq!(
        snapshot.fonts.family, FAMILY,
        "the fake fontconfig probe must answer before mounting"
    );
    cx.update(|cx| {
        snapshot.install(cx);
        cx.set_global(snapshot.fonts.typeface(&flea::uischema::defaults()));
    });
    (cx, recorder)
}

fn locale() -> crate::window::locale::Snapshot {
    crate::window::locale::Snapshot {
        revision: 2,
        catalog: Arc::new(flea::i18n::I18n::load(flea::i18n::Paths {
            omarchy: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../src/i18n/fixtures/omarchy"),
            language_file: std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap()
                .join("language"),
            catalogs: Vec::new(),
        })),
    }
}

fn open(cx: &mut TestAppContext) -> Handle {
    open_with(cx, |_, _, _| {})
}

fn open_with(
    cx: &mut TestAppContext,
    setup: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>),
) -> Handle {
    cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let backend = channel(cx);
            let mut shell = crate::state::tests::shell_with(backend, cx);
            shell.apply_locale(locale(), cx);
            shell.launch.settings.data = flea::uischema::defaults();
            shell.panes[0].as_mut().unwrap().pane.commit(0);
            shell.panes[0].as_mut().unwrap().pane.held = crate::list::bench::held(
                (0..8)
                    .map(|at| flea::protocol::RowFacts {
                        name: format!("file-{at}.txt"),
                        mode: 0o100644,
                        icon: "text-plain".into(),
                        ..Default::default()
                    })
                    .collect(),
            );
            shell.panes[0].as_mut().unwrap().changed(&shell.locale);
            shell.sync_font_settings(cx);
            window.focus(&shell.focus, cx);
            setup(&mut shell, window, cx);
            shell
        }),
    })
}

fn channel(cx: &mut Context<Shell>) -> crate::backend::Backend {
    let (backend, requests) = crate::backend::Backend::test_channel();
    cx.on_release(move |_, _| drop(requests)).detach();
    backend
}

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| change(shell, window, cx))
        })
        .unwrap()
}

fn assert_runs(surface: &str, recorder: &receipt::Recorder, required: &[&str]) -> bool {
    assert_family(surface, recorder, required, FAMILY)
}

// No board exempts GPUI text; file pixels, font specimens included, are not GPUI runs.
fn assert_family(
    surface: &str,
    recorder: &receipt::Recorder,
    required: &[&str],
    expected: &str,
) -> bool {
    let runs = recorder.finish();
    for run in &runs {
        println!(
            "FONT_RUN surface={surface} text={:?} requested={:?} resolved={:?}",
            run.text, run.requested, run.resolved
        );
    }
    let missing: Vec<_> = required
        .iter()
        .filter(|needle| !runs.iter().any(|run| run.text.contains(**needle)))
        .collect();
    if runs.is_empty() || !missing.is_empty() {
        println!(
            "FONT_GATE surface={surface} missing={missing:?} mounted={}",
            !runs.is_empty()
        );
    }
    let failures: Vec<_> = runs
        .iter()
        .filter(|run| {
            run.requested.is_empty()
                || run.requested.iter().any(|name| name != expected)
                || (run.resolved.is_empty() && !run.text.trim().is_empty())
                || run.resolved.iter().any(|(_, name)| name != expected)
        })
        .collect();
    for run in &failures {
        println!(
            "FONT_GATE surface={surface} text={:?} requested={:?} resolved={:?}",
            run.text, run.requested, run.resolved
        );
    }
    let passed = !runs.is_empty() && missing.is_empty() && failures.is_empty();
    println!(
        "FONT_GATE surface={surface} runs={} failures={} {}",
        runs.len(),
        failures.len(),
        if passed { "PASS" } else { "FAIL" }
    );
    passed
}

fn draw(cx: &mut TestAppContext, handle: Handle) {
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

#[test]
fn font_gate_every_surface_uses_system_family() {
    if isolation::enter(
        "font_gate::font_gate_every_surface_uses_system_family",
        &format!("#!/bin/sh\nprintf '%s' '{FAMILY}'\n"),
    ) {
        return;
    }
    let mut failed = surfaces::check();
    failed.extend(picker::check());
    failed.extend(readers::check());
    failed.extend(dialogs::check());
    failed.extend(settings::check());
    println!("FONT_GATE failed_surfaces={failed:?}");
    assert!(failed.is_empty(), "system font failures: {failed:?}");
}
