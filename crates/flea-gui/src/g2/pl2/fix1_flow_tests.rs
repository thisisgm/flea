//! Real browser events hand credentials to activation before retiring their view owner.
use super::{
    browser::{Activated, Closed, ShareView},
    dialog::{Dialog, Effect as DialogEffect},
    mount::{Effect, Mounts, Phase, Request},
};
use crate::g2::pl1::{process::Outcome, secret::Secret};
use std::{cell::RefCell, rc::Rc};

#[gpui::test]
fn fix1_browser_close_retires_credentials_after_activation_handoff(cx: &mut gpui::TestAppContext) {
    let (_root, catalog) = super::input_tests::catalog();
    let mounts = Rc::new(RefCell::new(Mounts::default()));
    let order = Rc::new(RefCell::new(Vec::new()));
    let view = cx.add_window(|_, cx| ShareView::new(catalog, super::input_tests::palette(), cx));
    let entity = view.update(cx, |_, _, cx| cx.entity()).unwrap();
    for activate in [true, false] {
        {
            let mut m = mounts.borrow_mut();
            m.stop_all();
            m.open_share(
                Request {
                    id: 0,
                    uri: "smb://user@host/".into(),
                    label: "Host".into(),
                    origin: 7,
                    mounted: false,
                    authenticated: true,
                    password: Secret::from_bytes(b"synthetic-secret"),
                },
                0,
            );
            let g = m.generation;
            m.exited(g, Phase::Auth, &Outcome::code(0), 1);
            m.exited(g, Phase::Info, &Outcome::code(0), 2);
            m.exited(
                g,
                Phase::Shares,
                &Outcome {
                    code: 0,
                    stdout: "media\n".into(),
                    stderr: String::new(),
                },
                3,
            );
        }
        assert!(mounts.borrow().has_credentials(7));
        let subscriptions = cx.update(|cx| {
            let a_mounts = mounts.clone();
            let a_order = order.clone();
            let activated = cx.subscribe(&entity, move |_, e: &Activated, _| {
                a_order.borrow_mut().push("activated");
                let effects = a_mounts.borrow_mut().open_child_share(e.0.clone(), e.1.clone(), e.2, 4);
                assert!(effects.iter().any(|e| matches!(e, Effect::Run { phase: Phase::Auth, job, .. } if job.stdin.as_ref().unwrap().expose() == b"synthetic-secret\n")));
            });
            let c_mounts = mounts.clone();
            let c_order = order.clone();
            let closed = cx.subscribe(&entity, move |_, e: &Closed, _| {
                c_order.borrow_mut().push("closed");
                c_mounts.borrow_mut().close_browser(e.0);
            });
            (activated, closed)
        });
        order.borrow_mut().clear();
        view.update(cx, |v, window, cx| {
            v.show(
                "smb://user@host/".into(),
                "Host".into(),
                vec!["media".into()],
                7,
                window,
                cx,
            );
            assert!(v.action(if activate { "open" } else { "escape" }, cx));
        })
        .unwrap();
        cx.run_until_parked();
        if activate {
            assert_eq!(&*order.borrow(), &["activated", "closed"]);
            assert!(
                mounts.borrow().has_credentials(7),
                "only the child operation now owns it"
            );
        } else {
            assert_eq!(&*order.borrow(), &["closed"]);
            assert!(!mounts.borrow().has_credentials(7));
        }
        drop(subscriptions);
    }
}

#[test]
fn fix1_retry_and_label_only_submission_keep_canonical_encoded_address() {
    let uri = "sftp://user@host/a%2Fb";
    let mut d = Dialog::default();
    d.open_location(uri, "Saved", true);
    d.form.label = "Renamed".into();
    d.password.push_str("synthetic-secret");
    assert!(matches!(d.submit().as_slice(), [DialogEffect::Mount { uri: got, .. }] if got == uri));
    d.mount_finished(d.request, uri, Err("network.refused"), true);
    assert_eq!(d.password.expose(), b"synthetic-secret");
    d.password.clear();
    d.password.push_str("replacement-secret");
    assert!(matches!(d.submit().as_slice(), [DialogEffect::Mount { uri: got, .. }] if got == uri));
}

#[test]
fn fix1_repair_keeps_no_folder_and_real_user_conflict_distinct_from_refusal() {
    for (mount_code, names, expected) in [
        (
            0,
            vec!["smb-share:server=other,share=wrong"],
            "network.no_folder",
        ),
        (
            1,
            vec![
                "smb-share:server=host,share=share,user=one",
                "smb-share:server=host,share=share,user=two",
            ],
            "network.mounted_as",
        ),
    ] {
        let mut m = Mounts::default();
        m.open_share(
            Request {
                id: 1,
                uri: "smb://host/share".into(),
                label: "Share".into(),
                origin: 7,
                mounted: false,
                authenticated: false,
                password: Secret::new(),
            },
            0,
        );
        let g = m.generation;
        m.exited(g, Phase::Mount, &Outcome::code(mount_code), 1);
        let wrong = "/run/user/1000/gvfs/smb-share:server=other,share=wrong";
        m.exited(
            g,
            Phase::Info,
            &Outcome {
                code: 0,
                stdout: format!("local path: {wrong}\n"),
                stderr: String::new(),
            },
            2,
        );
        m.peeked(g, wrong, &[], true, "third", 3);
        let names: Vec<String> = names.into_iter().map(str::to_owned).collect();
        let e = m.peeked(g, "/run/user/1000/gvfs", &names, false, "third", 4);
        assert!(e
            .iter()
            .any(|e| matches!(e, Effect::Completed { error: Some(key), .. } if *key == expected)));
    }
}
