//! Side presses preserve modal input and navigate only after that owner releases the chooser.
use super::{
    process::Line,
    side_button_tests::{landed, press},
    tests::fixture::{fixture, Fixture},
};
use gpui::{NavigationDirection, TestAppContext};

fn seed<'a>(cx: &'a mut TestAppContext, request: &str) -> Fixture<'a> {
    let mut f = fixture(cx, request, None);
    landed(&mut f, "/d");
    f.update(|p, cx| p.open_folder("/d/sub", cx));
    landed(&mut f, "/d/sub");
    f.update(|p, cx| p.open_folder("/d/sub/deep", cx));
    landed(&mut f, "/d/sub/deep");
    f.update(|p, cx| p.go_back(cx));
    landed(&mut f, "/d/sub");
    assert_eq!(f.read(|p| p.chrome.history.clone()), ["/d"]);
    assert_eq!(f.read(|p| p.chrome.ahead.clone()), ["/d/sub/deep"]);
    f
}

fn owner_preserved(cx: &mut TestAppContext, direction: NavigationDirection) {
    for owner in ["filter", "path", "folder", "collision", "submitting"] {
        let mut f = seed(
            cx,
            r#"{"mode":"save","name":"a.txt","folder":"/d","filters":[{"label":"Text","globs":["*.txt"]}]}"#,
        );
        let picker = f.picker.clone();
        f.cx.update(|window, cx| {
            picker.update(cx, |p, cx| {
                window.focus(&p.focus, cx);
                match owner {
                    "filter" => p.open_filter(window, cx),
                    "path" => p
                        .browse
                        .path_view(cx)
                        .update(cx, |path, cx| path.open(window, cx)),
                    "folder" => p.open_new_folder(window, cx),
                    "collision" => {
                        p.review = Some(flea::ui_logic_picker_portal::SaveReview {
                            folder: "/d/sub".into(),
                            name: "a.txt".into(),
                            collision: true,
                            review: 1,
                            path: "/d/sub/a.txt".into(),
                        });
                        p.review_asked = true;
                        cx.notify();
                    }
                    "submitting" => {
                        p.accept_marks = true;
                        cx.notify();
                    }
                    _ => unreachable!(),
                }
            })
        });
        f.cx.run_until_parked();
        if matches!(owner, "path" | "folder") {
            f.cx.simulate_input("keep-input");
            f.cx.run_until_parked();
        }
        let field_text = |p: &super::Picker, cx: &gpui::App| match owner {
            "path" => p.browse.path_view(cx).read(cx).entry.text.clone(),
            "folder" => p.chrome.folder.as_ref().unwrap().read(cx).text().to_owned(),
            _ => String::new(),
        };
        let input = f.update(|p, cx| field_text(p, cx));
        let focus = f.cx.update(|w, cx| w.focused(cx));
        let trails = f.read(|p| (p.chrome.history.clone(), p.chrome.ahead.clone()));
        let requests = (f.listing.sent().len(), f.checks.sent().len());
        press(&mut f, direction);
        assert_eq!(f.read(|p| p.listing.location.clone()), "/d/sub", "{owner}");
        assert_eq!(
            f.read(|p| (p.chrome.history.clone(), p.chrome.ahead.clone())),
            trails,
            "{owner}"
        );
        assert_eq!(
            (f.listing.sent().len(), f.checks.sent().len()),
            requests,
            "{owner}"
        );
        assert_eq!(f.cx.update(|w, cx| w.focused(cx)), focus, "{owner}");
        assert_eq!(f.update(|p, cx| field_text(p, cx)), input, "{owner}");
        assert_eq!(f.read(|p| p.listing.save_name.clone()), "a.txt", "{owner}");
        assert!(
            f.written().is_none(),
            "{owner}: no acceptance under pointer"
        );
        match owner {
            "filter" => assert!(f.read(|p| p.chrome.menu_view.is_some())),
            "path" => assert!(f.update(|p, cx| p.browse.path_view(cx).read(cx).editing())),
            "folder" => assert!(f.read(|p| p.chrome.folder.is_some())),
            "collision" => assert!(f.read(|p| p.accept_state().collision_review())),
            "submitting" => assert!(f.read(|p| p.accept_state().submitting())),
            _ => unreachable!(),
        }
        f.update(|p, cx| {
            p.chrome.menu = None;
            p.chrome.menu_view = None;
            p.chrome.folder = None;
            p.review_asked = false;
            p.accept_marks = false;
            p.browse.path_view(cx).update(cx, |path, cx| path.close(cx));
            cx.notify();
        });
        f.cx.run_until_parked();
        press(&mut f, direction);
        let destination = match direction {
            NavigationDirection::Back => "/d",
            NavigationDirection::Forward => "/d/sub/deep",
        };
        assert_eq!(
            f.read(|p| p.listing.location.clone()),
            destination,
            "{owner}: released owner permits real travel"
        );
        assert!(f.written().is_none());
    }
}

#[gpui::test]
fn side_back_preserves_each_input_owner(cx: &mut TestAppContext) {
    owner_preserved(cx, NavigationDirection::Back);
}

#[gpui::test]
fn side_forward_preserves_each_input_owner(cx: &mut TestAppContext) {
    owner_preserved(cx, NavigationDirection::Forward);
}

fn file_row(f: &mut Fixture) {
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"replace.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"}]}"#.into()));
    f.cx.run_until_parked();
}

#[gpui::test]
fn side_button_does_not_activate_or_rename_a_file_row(cx: &mut TestAppContext) {
    for save in [false, true] {
        let request = if save {
            r#"{"mode":"save","name":"keep.txt","folder":"/d"}"#
        } else {
            r#"{"folder":"/d"}"#
        };
        let mut f = seed(cx, request);
        for (direction, destination) in [
            (NavigationDirection::Back, "/d"),
            (NavigationDirection::Forward, "/d/sub"),
        ] {
            file_row(&mut f);
            press(&mut f, direction);
            assert_eq!(f.read(|p| p.listing.location.clone()), destination);
            assert!(
                f.written().is_none(),
                "side button must not accept the file row"
            );
            if save {
                assert_eq!(f.read(|p| p.listing.save_name.clone()), "keep.txt");
            }
            landed(&mut f, destination);
        }
    }
}
