use crate::{
    row::format::{day_start, is_recent},
    window::{columns_tests::host, startup::View},
};

#[test]
fn highlight_today_setting_reaches_the_drawn_list() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::List, window, cx);
        shell.panes[0].as_mut().unwrap().pane.held =
            crate::list::bench::held(vec![host::row("notes.txt", false)]);
    });
    for (document, on) in [
        ("{}", false),
        (r#"{"highlightToday":true}"#, true),
        ("{}", false),
    ] {
        host::change(&mut cx, handle, |shell, window, cx| {
            shell.apply_settings(flea::uistate::from_file(document), window, cx);
        });
        host::frame(&mut cx, handle);
        host::change(&mut cx, handle, |shell, _, _| {
            let today = shell.panes[0].as_ref().unwrap().list.borrow().view.today;
            assert_eq!(today.is_some(), on, "runtime settings {document}");
            assert_eq!(
                shell.midnight.is_some(),
                on,
                "the midnight wake follows {document}"
            );
            if let Some(today) = today {
                assert!(is_recent(now(), today), "a stamp from now is today's");
                assert!(
                    !is_recent(today - 1, today),
                    "a stamp before midnight is not"
                );
            }
        });
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[test]
fn highlight_today_moves_to_the_new_day_only_once_the_old_midnight_has_passed() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::List, window, cx);
        shell.panes[0].as_mut().unwrap().pane.held =
            crate::list::bench::held(vec![host::row("notes.txt", false)]);
        let on = flea::uistate::from_file(r#"{"highlightToday":true}"#);
        shell.apply_settings(on, window, cx);
    });
    let (midnight, recent, shortest) = (day_start(now()), now() - 60, now() - 23 * 3600);
    // The list keeps a stored midnight while it is under 23 h old (the shortest local day) and replaces it after.
    for (seeded, expected) in [
        (midnight - 25 * 3600, midnight),
        (shortest - 30, midnight),
        (shortest + 30, shortest + 30),
        (recent, recent),
    ] {
        host::change(&mut cx, handle, |shell, _, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow_mut()
                .view
                .today = Some(seeded);
        });
        host::frame(&mut cx, handle);
        host::change(&mut cx, handle, |shell, _, _| {
            let today = shell.panes[0].as_ref().unwrap().list.borrow().view.today;
            assert_eq!(today, Some(expected), "seeded {seeded}");
        });
    }
}
