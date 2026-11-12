//! S1A-3: the strip keys answer from the rail as they do from the list.
use super::*;

#[gpui::test]
fn fx462_middle_click_retries_a_favourite_with_a_cached_status(cx: &mut TestAppContext) {
    let (shell, cx) = open_places(
        cx,
        "default",
        "list",
        r#","favourites":[{"label":"Late","path":"/returned"}]"#,
    );
    land(&shell, cx, "/files", home(), 1);
    shell.update(cx, |shell, cx| {
        let producer = shell.rail.as_ref().unwrap().places.clone();
        producer.update(cx, |producer, cx| {
            producer.places.update(cx, |places, cx| {
                // An empty answer from /bin/true is unreadable, so only a retried inspection sets `said`.
                places.exe = "/bin/true".into();
                places.said = None;
                places
                    .favourites
                    .statuses
                    .insert(0, "unavailable (NotFound)".into());
                cx.notify();
            });
        });
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    let retried = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, cx| {
            let producer = shell.rail.as_ref().unwrap().places.read(cx);
            producer.places.read(cx).said.is_some()
        })
    };
    assert!(!retried(&shell, cx), "an inspection ran before the press");
    assert!(read(&shell, cx, "railEntries", &[]).contains("unavailable (NotFound)"));
    let centre = rail_centre(&shell, cx, "Late");
    middle(cx, &centre);
    assert!(
        retried(&shell, cx),
        "the middle click did not retry the cached status"
    );
    assert_eq!(read(&shell, cx, "tabCount", &[]), "2");
    assert_eq!(asked(&shell, cx), "/returned");
    close(&shell, cx);
}

// S1A-3: with the keyboard in the rail a digit selects its tab and names a missing one, as it does from the list.
#[gpui::test]
fn a_digit_in_the_rail_selects_its_tab_and_names_a_missing_one(cx: &mut TestAppContext) {
    let (shell, cx) = two_tabs(cx, "default");
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    cx.simulate_keystrokes("tab");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.keyboard.context),
        Context::Rail
    );
    digit(cx, "1");
    assert_eq!(strip(&shell, cx).1, "0");
    assert_eq!(asked(&shell, cx), "/files");
    land(&shell, cx, "/files", home(), 3);
    digit(cx, "2");
    assert_eq!(strip(&shell, cx).1, "1");
    land(&shell, cx, "/files/alpha", vec![row("inner", true)], 4);
    digit(cx, "5");
    assert_eq!(message(&shell, cx), "No tab 5");
    assert_eq!(strip(&shell, cx).1, "1");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.keyboard.context),
        Context::Rail,
        "a tab key leaves the keyboard in the rail"
    );
    close(&shell, cx);
}
