use super::{
    dialog::{Dialog, Effect, Focus},
    mount::{self, Mounts, Phase, Request},
};
use crate::g2::device_rail::{process::Outcome, secret::Secret};
use flea::ui_logic_network_shares::form::Protocol;

fn draft() -> Dialog {
    let mut d = Dialog::default();
    d.open();
    d.form.host = "fixture.test".into();
    d.form.path = "share".into();
    d.password.push_str("fixture-password");
    d
}
fn request(uri: &str) -> Request {
    Request {
        id: 1,
        uri: uri.into(),
        label: "Fixture".into(),
        origin: 7,
        mounted: false,
        authenticated: false,
        password: Secret::new(),
    }
}

#[test]
fn pl2_dialog_mounts_once_and_refuses_stale_or_wrong_endpoint() {
    let mut d = draft();
    assert!(matches!(d.submit().as_slice(), [Effect::Mount { .. }]));
    assert!(d.submit().is_empty());
    let id = d.request;
    let uri = d.pending_uri.clone();
    assert!(d.mount_finished(id + 1, &uri, Ok(()), false).is_empty());
    assert!(d
        .mount_finished(id, "smb://other/share", Ok(()), false)
        .is_empty());
    assert!(d.connecting);
    assert!(d
        .mount_finished(id, &uri, Err("network.refused"), true)
        .is_empty());
    assert!(d.opened && d.retry && d.failed_connect && !d.saving);
    assert!(
        d.password.expose() == b"fixture-password",
        "the open Retry form retains its masked draft"
    );
}

#[test]
fn pl2_save_retry_does_not_remount_and_committed_response_cannot_repeat() {
    let mut d = draft();
    d.submit();
    let id = d.request;
    let uri = d.pending_uri.clone();
    assert!(matches!(
        d.mount_finished(id, &uri, Ok(()), false).as_slice(),
        [Effect::Save { .. }]
    ));
    assert!(d.saved(id + 1, true, "").is_empty());
    assert!(d.saving);
    assert!(d.close().is_empty());
    assert!(d.opened);
    d.saved(id, false, "Store could not be written");
    assert!(matches!(d.submit().as_slice(), [Effect::Save { .. }]));
    let id = d.request;
    d.saved(id, true, "Saved, state unreadable");
    assert!(d.committed && d.opened);
    assert!(d.submit().iter().any(|e| matches!(e, Effect::Closed)));
    assert!(!d.opened);
    assert!(d.password.is_empty());
    assert!(d.saved(id, true, "").is_empty());
}

#[test]
fn pl2_cancel_and_backend_death_erase_draft_secrets_and_ignore_completion() {
    let mut d = draft();
    d.submit();
    let id = d.request;
    let uri = d.pending_uri.clone();
    assert!(d
        .close()
        .iter()
        .any(|e| matches!(e, Effect::Cancel(n) if *n == id)));
    assert!(d.password.is_empty());
    assert!(d.mount_finished(id, &uri, Ok(()), false).is_empty());
    d.open();
    d.password.push_str("private");
    d.backend_died();
    assert!(d.password.is_empty());
    assert!(!d.busy());
    assert!(d.provider_check.is_none());
}

#[test]
fn pl2_open_does_not_overwrite_a_live_draft_and_existing_never_saves() {
    let mut d = draft();
    d.form.label = "Keep".into();
    assert!(d.open_location("smb://other/", "Other", false).is_empty());
    assert_eq!(d.form.label, "Keep");
    d.close();
    d.open_location("smb://other/", "Other", false);
    d.submit();
    let id = d.request;
    let uri = d.pending_uri.clone();
    assert!(d
        .mount_finished(id, &uri, Ok(()), false)
        .iter()
        .any(|e| matches!(e, Effect::Closed)));
    assert!(!d.opened);
}

#[test]
fn pl2_invalid_form_and_missing_password_do_no_work() {
    let mut d = draft();
    d.form.port = "65536".into();
    assert!(d.submit().is_empty());
    assert_eq!(d.status, Some("network.invalid_form"));
    d.form.port = "445".into();
    d.form.user = "user".into();
    d.password.clear();
    assert!(d.submit().is_empty());
    assert_eq!(d.status, Some("network.password_needed"));
    d.pick(4);
    assert!(!d.ring().contains(&Focus::Password));
    assert!(matches!(d.submit().as_slice(), [Effect::Mount { .. }]));
}

#[test]
fn pl2_opening_and_closing_ask_for_no_hosts_and_the_card_takes_no_list() {
    let mut d = Dialog::default();
    let opened = d.open();
    let id = d.provider_check.unwrap();
    assert!(
        matches!(opened.as_slice(), [Effect::CheckDropbox(check)] if *check == id),
        "opening sends the provider check alone"
    );
    d.step(false);
    assert_eq!(d.focus, Focus::Port, "Tab walks the fields, never a list");
    let closed = d.close();
    assert!(
        matches!(closed.as_slice(), [Effect::CancelDropbox(cancel), Effect::Closed] if *cancel == id),
        "closing sends no discovery stop"
    );
}

#[test]
fn pl2_key_first_and_info_runs_even_after_mount_refusal() {
    let mut m = Mounts::default();
    let r = request("sftp://user@host:22/home/");
    let e = m.open_share(r, 0);
    let generation = m.generation;
    assert!(
        matches!(&e[0], mount::Effect::Run { phase: Phase::Mount, job, .. } if job.argv == ["gio", "mount", "--", "sftp://user@host/home"] && job.stdin.is_none())
    );
    assert!(matches!(
        m.exited(generation, Phase::Mount, &Outcome::code(1), 10)
            .as_slice(),
        [mount::Effect::Run {
            phase: Phase::Info,
            ..
        }]
    ));
    let e = m.exited(generation, Phase::Info, &Outcome::code(1), 20);
    assert!(matches!(
        e.as_slice(),
        [
            mount::Effect::Completed {
                error: Some("network.password_needed"),
                ..
            },
            mount::Effect::Say { origin: 7, .. }
        ]
    ));
}

#[test]
fn pl2_mount_overlap_browser_origin_and_stale_exits() {
    let mut m = Mounts::default();
    let mut r = request("smb://first/");
    r.mounted = true;
    m.open_share(r, 0);
    let generation = m.generation;
    m.exited(generation, Phase::Info, &Outcome::code(1), 10);
    let busy = m.open_share(request("smb://second/"), 11);
    assert!(matches!(
        busy.as_slice(),
        [mount::Effect::Completed {
            error: Some("network.another_opening"),
            ..
        }]
    ));
    assert!(m
        .exited(generation + 1, Phase::Shares, &Outcome::default(), 12)
        .is_empty());
    let out = Outcome {
        stdout: "docs\nmedia\n".into(),
        ..Outcome::default()
    };
    let e = m.exited(generation, Phase::Shares, &out, 13);
    assert!(e.iter().any(|e| matches!(e, mount::Effect::Shares { names, origin: 7, .. } if names == &["docs", "media"])));
    assert!(!m.busy());
}

#[test]
fn pl2_auth_secret_stays_off_argv_and_errors_use_status() {
    for (code, key) in [
        (124, "network.no_answer"),
        (126, "network.no_prompt"),
        (127, "network.no_prompt"),
        (-1, "network.no_prompt"),
        (77, "network.password_refused"),
        (1, "network.failed"),
        (60, "network.failed"),
    ] {
        let mut m = Mounts::default();
        let mut r = request("smb://gm@nas/share");
        r.password.push_str("fixture-secret");
        let e = m.open_share(r, 0);
        assert!(!format!("{e:?}").contains("fixture-secret"));
        let mount::Effect::Run { job, .. } = &e[0] else {
            panic!()
        };
        assert_eq!(job.stdin.as_ref().unwrap().expose(), b"fixture-secret\n");
        assert_eq!(job.env, [("LC_ALL".into(), "C".into())]);
        let e = m.exited(
            m.generation,
            Phase::Auth,
            &Outcome {
                code,
                stderr: "translated unrelated text".into(),
                stdout: String::new(),
            },
            2,
        );
        assert!(matches!(&e[0], mount::Effect::Completed { error: Some(k), .. } if *k == key));
    }
}

#[test]
fn pl2_timeout_and_cancel_release_single_flight_and_refuse_late_answers() {
    let mut m = Mounts::default();
    m.open_share(request("smb://nas/"), 0);
    let generation = m.generation;
    assert!(m.tick(14_999).is_empty());
    assert!(!m.tick(15_000).is_empty());
    assert!(!m.busy());
    assert!(m
        .exited(generation, Phase::Mount, &Outcome::code(0), 15_001)
        .is_empty());
    m.open_share(request("smb://new/"), 16_000);
    let generation = m.generation;
    assert!(m.cancel(99).is_empty());
    assert!(m.busy());
    assert!(!m.cancel(1).is_empty());
    assert!(m
        .exited(generation, Phase::Mount, &Outcome::code(0), 16_001)
        .is_empty());
}

#[test]
fn pl2_smb_repair_keeps_the_original_pane_and_checks_paths_before_bridge() {
    let mut m = Mounts::default();
    let mut r = request("smb://dan@nas/test");
    r.mounted = true;
    m.open_share(r, 0);
    let g = m.generation;
    let path = "/run/user/1000/gvfs/smb-share:server=nas,share=test";
    let out = Outcome {
        stdout: format!("local path: {path}\n"),
        ..Outcome::default()
    };
    assert!(matches!(
        m.exited(g, Phase::Info, &out, 1).as_slice(),
        [mount::Effect::Peek { count: 1, .. }]
    ));
    assert!(m.peeked(g, "/other", &[], false, "gm", 2).is_empty());
    assert!(matches!(
        m.peeked(g, path, &[], true, "gm", 2).as_slice(),
        [mount::Effect::Peek { count: 512, .. }]
    ));
    let names = ["smb-share:server=nas,share=test,user=dan".into()];
    let e = m.peeked(g, "/run/user/1000/gvfs", &names, false, "gm", 3);
    assert!(e.iter().any(
        |e| matches!(e, mount::Effect::Open { origin: 7, path, .. } if path.ends_with(",user=dan"))
    ));
}

#[gpui::test]
fn pl2_real_entity_and_element_draw_all_protocols(cx: &mut gpui::TestAppContext) {
    use crate::{g2::file_ops::palette::CardPalette, list::Palette, palette::Theme};
    use flea::i18n::{I18n, Paths};
    use gpui::prelude::*;
    use std::{sync::Arc, time::Duration};
    let root = super::store_tests::Root::new();
    let omarchy = root.0.join("omarchy");
    std::fs::create_dir_all(omarchy.join("default/i18n/en")).unwrap();
    std::fs::write(
        omarchy.join("default/i18n/en/shell.json"),
        r#"{"common.cancel":"Cancel"}"#,
    )
    .unwrap();
    let catalog = Arc::new(I18n::load(Paths {
        omarchy,
        language_file: "/nonexistent".into(),
        catalogs: vec![crate::test_catalogs::english_only()],
    }));
    let handle = cx.add_window(|_, cx| {
        let p = Palette::inherited(&gpui::TextStyle::default(), Theme::default());
        super::view::NetworkView::new(catalog, CardPalette::from_list(&p, 8.), cx)
    });
    handle
        .update(cx, |v, w, cx| {
            v.model.dropbox_installed = true;
            v.open(w, cx);
        })
        .unwrap();
    for protocol in [
        Protocol::Smb,
        Protocol::Sftp,
        Protocol::Ftps,
        Protocol::WebDav,
        Protocol::Nfs,
    ] {
        handle
            .update(cx, |v, _, cx| {
                v.model.form.pick(protocol);
                v.changed(cx);
            })
            .unwrap();
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        })
        .unwrap();
        handle
            .update(cx, |v, window, _| {
                let d = v.drawn.borrow();
                assert!(!d.text.is_empty());
                assert!(d.boxes.len() >= 7);
                assert_eq!(
                    d.card.unwrap().h,
                    super::drawing::HEIGHT.min(f32::from(window.viewport_size().height) - 16.)
                );
            })
            .unwrap();
    }
}

#[path = "fix_fileops_2_tests.rs"]
mod fix_fileops_2;
