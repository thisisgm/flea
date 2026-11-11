use super::*;

#[gpui::test]
fn one_read_runs_and_a_newer_ask_replaces_the_one_waiting(cx: &mut gpui::TestAppContext) {
    let made = std::process::Command::new("mktemp")
        .args(["-d", "/tmp/flea-sniff-queue.XXXXXXXX"])
        .output()
        .unwrap();
    assert!(made.status.success());
    let dir = std::path::PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
    std::fs::write(dir.join(".flea-test-sandbox"), "sniff queue test").unwrap();
    let file = |name: &str| {
        let path = dir.join(name).to_string_lossy().into_owned();
        std::fs::write(&path, "words\n").unwrap();
        (path, 6u64, 0i64)
    };
    let (a, b, c) = (file("a"), file("b"), file("c"));
    let heard = std::rc::Rc::new(RefCell::new(Vec::new()));
    cx.update(|cx| {
        for key in [&a, &b, &c] {
            let (heard, name) = (heard.clone(), key.0.clone());
            ask(key.clone(), cx, move |_| heard.borrow_mut().push(name));
        }
    });
    cx.run_until_parked();
    // a ran at once; b waited and c took its place, so b was never read and never heard.
    assert_eq!(*heard.borrow(), [a.0.clone(), c.0.clone()]);
    assert_eq!(
        (answer(&a), answer(&b), answer(&c)),
        (Some(Head::Text), None, Some(Head::Text))
    );
    assert!(
        dir.is_absolute() && dir.starts_with("/tmp") && dir.join(".flea-test-sandbox").is_file()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
