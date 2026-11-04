//! Typed-address card labels and mounted-path regression cases.
use super::*;

// EY-396: v0.3.12's failed typed-address card left LABEL empty and drew the share name as the muted suggestion.
#[gpui::test]
fn ey396_failed_typed_address_card_suggests_the_share_name(cx: &mut TestAppContext) {
    for (uri, name) in [
        ("sftp://127.0.0.1/var/lib/flea-dev/sftp/share", "share"),
        ("smb://127.0.0.1/flea", "flea"),
        ("nfs://127.0.0.1/nfs", "nfs"),
    ] {
        let recorder = Rc::new(Recorder::default());
        let (shell, cx, _lines) = open(recorder.clone(), cx);
        cx.simulate_keystrokes("ctrl-l ctrl-a");
        cx.simulate_input(uri);
        cx.simulate_keystrokes("enter");
        frame(cx);
        let at = *recorder.mounts().last().expect("the address mounts");
        recorder.answer(at, 1, "");
        cx.run_until_parked();
        let at = *recorder
            .find("info")
            .last()
            .expect("the mount asks for its folder");
        recorder.answer(at, 1, "");
        frame(cx);
        assert!(
            !call(&shell, "networkCardRect", &[], cx).is_empty(),
            "{uri}: the failed address opens its repair card"
        );
        card(&shell, cx).read_with(cx, |card, _| {
            assert!(card.model.retry, "{uri}");
            assert_eq!(card.model.form.host, "127.0.0.1", "{uri}");
            assert_eq!(
                card.model.form.label, "",
                "{uri}: LABEL holds no typed text"
            );
            assert_eq!(card.model.form.name(), name, "{uri}: the suggestion");
        });
    }
}

// EY-456: a typed share records no crumb alias (v0.3.12 kept the mounted path), while the browser's Open names its share.
#[gpui::test]
fn ey456_typed_share_keeps_the_mounted_path_in_the_bar(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder.clone(), cx);
    cx.simulate_keystrokes("ctrl-l ctrl-a");
    cx.simulate_input("smb://127.0.0.1/flea");
    cx.simulate_keystrokes("enter");
    frame(cx);
    let mounted = "/fixture/gvfs/smb-share:server=127.0.0.1,share=flea";
    answer_mount(&recorder, mounted, cx);
    shell.read_with(cx, |shell, _| {
        let place = shell.panes[0].as_ref().unwrap().pane.place();
        assert_eq!(place, mounted, "the typed share opened");
    });
    let labels = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| {
            shell
                .chrome
                .aliases
                .iter()
                .map(|alias| alias.label.clone())
                .collect::<Vec<_>>()
        })
    };
    assert!(
        labels(&shell, cx).is_empty(),
        "a typed share records no crumb alias, so the bar keeps the mounted path"
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let origin = shell.network_origin(0);
            shell.network_service(
                &Effect::Open {
                    path: mounted.into(),
                    uri: "smb://127.0.0.1/flea/".into(),
                    label: "flea".into(),
                    origin,
                },
                window,
                cx,
            );
        })
    });
    frame(cx);
    assert_eq!(
        labels(&shell, cx),
        ["flea"],
        "the browser's Open names its share"
    );
}
