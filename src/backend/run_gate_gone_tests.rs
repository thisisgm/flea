// S3-7: the open folder, or one above it, deleted or moved away, driven through the same Loop as run_gate_tests.rs.
use super::*;

// Feeds the loop every event that reaches its channel until a line says `t` or the wait runs out, and returns all it said.
fn said_until(l: &mut Loop, t: &str, wait: Duration) -> String {
    let deadline = std::time::Instant::now() + wait;
    let mut said = String::new();
    while !said
        .lines()
        .any(|line| field_str(line, "t").as_deref() == Some(t))
    {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        let Ok(event) = l.rx.recv_timeout(left) else {
            break;
        };
        l.feed(event);
        said.push_str(&l.said());
    }
    said
}

// How long the loop is fed after its answer, so a second line from the other watcher would land before the test counts.
const QUIET: Duration = Duration::from_millis(800);

fn lines_of<'a>(said: &'a str, t: &str) -> Vec<&'a str> {
    said.lines()
        .filter(|line| field_str(line, "t").as_deref() == Some(t))
        .collect()
}

// The first gone line, then everything the loop says in the quiet window after it.
fn gone_and_after(l: &mut Loop) -> String {
    let mut said = said_until(l, "gone", EVENT_DEADLINE);
    said.push_str(&said_until(l, "never", QUIET));
    said
}

// Lists each folder a gone line names until one is still there, as a client does, and answers where the pane lands.
fn landing(l: &mut Loop, said: &mut String) -> String {
    let last_parent = |said: &str| {
        lines_of(said, "gone")
            .last()
            .and_then(|line| field_str(line, "parent"))
            .expect("a gone line")
            .to_string()
    };
    let mut parent = last_parent(said);
    while !Path::new(&parent).is_dir() {
        assert!(l.request(&list(Path::new(&parent))) == Control::Continue);
        said.push_str(&l.said());
        let next = last_parent(said);
        // Each gone line names a folder higher up, so a list that answers none would loop forever.
        assert_ne!(
            next, parent,
            "listing {parent} answered no gone line: {said}"
        );
        parent = next;
    }
    parent
}

fn gone_for<'a>(said: &'a str, path: &Path) -> Vec<&'a str> {
    lines_of(said, "gone")
        .into_iter()
        .filter(|line| field_str(line, "path") == Some(text(path)))
        .collect()
}

// Sample fdinfo line: "inotify wd:3 ino:1e0a4 sdev:800003 mask:fc6 ignored_mask:0 fhandle-bytes:8 fhandle-type:1 f_handle:..."
fn watches_on(fd: i32, folder: &Path) -> usize {
    use std::os::unix::fs::MetadataExt;
    let ino = std::fs::metadata(folder)
        .expect("the sandbox folder is there")
        .ino();
    let info = std::fs::read_to_string(format!("/proc/self/fdinfo/{fd}"))
        .expect("the loop's inotify fd has fdinfo");
    info.lines()
        .filter_map(|line| line.strip_prefix("inotify "))
        .filter(|line| {
            line.split(' ').any(|field| {
                field
                    .strip_prefix("ino:")
                    .and_then(|hex| u64::from_str_radix(hex, 16).ok())
                    == Some(ino)
            })
        })
        .count()
}

// A local folder lists inline, so the request itself lands the listing and arms both watches.
fn open_listed(l: &mut Loop, path: &Path) {
    assert!(l.request(&list(path)) == Control::Continue);
    let said = l.said();
    assert_eq!(
        field_str(said.lines().next().unwrap_or(""), "t").as_deref(),
        Some("listed"),
        "{said}"
    );
    // The follower records the folder on its own thread; the check it answers after that proves a test's move cannot beat it.
    l.watch.ancestry.check();
    let armed = said_until(l, "changed", EVENT_DEADLINE);
    assert_eq!(lines_of(&armed, "changed").len(), 1, "{armed}");
}

#[test]
fn deleting_the_folder_above_the_open_one_moves_the_pane_to_the_nearest_folder_still_there() {
    let mut l = Loop::new("rungate-gone-delete");
    let open = l.dir.dir("parent/open-me");
    l.dir.file("parent/open-me/a.txt", "a");
    open_listed(&mut l, &open);
    std::fs::remove_dir_all(l.dir.join("parent")).expect("the sandbox deletes its own folders");
    let mut said = gone_and_after(&mut l);
    assert_eq!(
        gone_for(&said, &open).len(),
        1,
        "one gone line for the open folder: {said}"
    );
    // The follower may judge between the two removals and name "parent" first; that list then answers its own gone.
    assert_eq!(
        landing(&mut l, &mut said),
        text(l.dir.path()),
        "the nearest folder still there: {said}"
    );
    assert!(lines_of(&said, "error").is_empty(), "{said}");
}

#[test]
fn moving_the_open_folder_away_moves_the_pane_to_its_parent() {
    let mut l = Loop::new("rungate-gone-move");
    let (parent, open) = (l.dir.dir("parent"), l.dir.dir("parent/open-me"));
    open_listed(&mut l, &open);
    std::fs::rename(&open, l.dir.join("renamed-away")).expect("the sandbox moves its own folder");
    let said = gone_and_after(&mut l);
    let gone = lines_of(&said, "gone");
    assert_eq!(gone.len(), 1, "one gone line: {said}");
    assert_eq!(field_str(gone[0], "path"), Some(text(&open)), "{said}");
    assert_eq!(field_str(gone[0], "parent"), Some(text(&parent)), "{said}");
}

#[test]
fn moving_a_folder_above_away_moves_the_pane_to_the_nearest_folder_still_there() {
    let mut l = Loop::new("rungate-gone-above");
    let open = l.dir.dir("top/mid/open-me");
    open_listed(&mut l, &open);
    std::fs::rename(l.dir.join("top/mid"), l.dir.join("moved"))
        .expect("the sandbox moves its own folder");
    let said = gone_and_after(&mut l);
    let gone = lines_of(&said, "gone");
    assert_eq!(gone.len(), 1, "one gone line: {said}");
    assert_eq!(field_str(gone[0], "path"), Some(text(&open)), "{said}");
    assert_eq!(
        field_str(gone[0], "parent"),
        Some(text(&l.dir.join("top"))),
        "{said}"
    );
}

// The client re-lists on the changed line a delete starts with; that list answers gone, never an unreadable folder.
#[test]
fn a_re_list_of_the_deleted_open_folder_answers_gone_once_and_no_error() {
    let mut l = Loop::new("rungate-gone-relist");
    let open = l.dir.dir("parent/open-me");
    open_listed(&mut l, &open);
    std::fs::remove_dir(&open).expect("the sandbox deletes its own empty folder");
    assert!(l.request(&list(&open)) == Control::Continue);
    let mut said = l.said();
    assert_eq!(
        lines_of(&said, "gone").len(),
        1,
        "the re-list itself answers gone: {said}"
    );
    said.push_str(&said_until(&mut l, "never", QUIET));
    let gone = lines_of(&said, "gone");
    assert_eq!(
        gone.len(),
        1,
        "the re-list's answer, and the follower's own finding is not said again: {said}"
    );
    assert_eq!(
        field_str(gone[0], "parent"),
        Some(text(&l.dir.join("parent"))),
        "{said}"
    );
    assert!(lines_of(&said, "error").is_empty(), "{said}");
}

// A file put where the open folder was fails the re-list as "not a folder"; that list answers gone too, never an error.
#[test]
fn a_re_list_of_the_open_folder_replaced_by_a_file_answers_gone_and_no_error() {
    let mut l = Loop::new("rungate-gone-file");
    let open = l.dir.dir("parent/open-me");
    open_listed(&mut l, &open);
    std::fs::remove_dir(&open).expect("the sandbox deletes its own empty folder");
    l.dir.file("parent/open-me", "now a file");
    assert!(l.request(&list(&open)) == Control::Continue);
    let mut said = l.said();
    assert_eq!(
        lines_of(&said, "gone").len(),
        1,
        "the ENOTDIR re-list itself answers gone: {said}"
    );
    said.push_str(&said_until(&mut l, "never", QUIET));
    let gone = lines_of(&said, "gone");
    assert_eq!(gone.len(), 1, "{said}");
    assert_eq!(
        field_str(gone[0], "parent"),
        Some(text(&l.dir.join("parent"))),
        "{said}"
    );
    assert!(lines_of(&said, "error").is_empty(), "{said}");
}

// The parent a gone line named can go too before the client lists it; that list answers gone in turn.
#[test]
fn the_folder_a_gone_line_named_going_too_answers_its_own_gone() {
    let mut l = Loop::new("rungate-gone-chain");
    let open = l.dir.dir("a/b/open-me");
    open_listed(&mut l, &open);
    std::fs::remove_dir(&open).expect("the sandbox deletes its own empty folder");
    let first = said_until(&mut l, "gone", EVENT_DEADLINE);
    assert_eq!(
        lines_of(&first, "gone")
            .first()
            .and_then(|line| field_str(line, "parent")),
        Some(text(&l.dir.join("a/b"))),
        "{first}"
    );
    std::fs::remove_dir_all(l.dir.join("a")).expect("the sandbox deletes its own folders");
    assert!(l.request(&list(&l.dir.join("a/b"))) == Control::Continue);
    let said = l.said();
    let gone = lines_of(&said, "gone");
    assert_eq!(gone.len(), 1, "{said}");
    assert_eq!(
        field_str(gone[0], "path"),
        Some(text(&l.dir.join("a/b"))),
        "{said}"
    );
    assert_eq!(
        field_str(gone[0], "parent"),
        Some(text(l.dir.path())),
        "{said}"
    );
    assert!(lines_of(&said, "error").is_empty(), "{said}");
}

// The TUI re-lists on the changed line an rm -r starts with, and that list can cross the gone line on the wire.
#[test]
fn a_re_list_that_crossed_the_gone_line_answers_that_line_again_and_no_error() {
    let mut l = Loop::new("rungate-gone-crossed");
    let open = l.dir.dir("parent/open-me");
    open_listed(&mut l, &open);
    std::fs::remove_dir(&open).expect("the sandbox deletes its own empty folder");
    let first = said_until(&mut l, "gone", EVENT_DEADLINE);
    assert_eq!(lines_of(&first, "gone").len(), 1, "{first}");
    assert!(l.request(&list(&open)) == Control::Continue);
    let said = l.said();
    assert_eq!(
        lines_of(&said, "gone"),
        lines_of(&first, "gone"),
        "the same line again: {said}"
    );
    assert!(lines_of(&said, "error").is_empty(), "{said}");
    // A folder the client was entering inside the gone one crossed the line the same way.
    assert!(l.request(&list(&open.join("sub"))) == Control::Continue);
    let said = l.said();
    assert_eq!(
        lines_of(&said, "gone"),
        lines_of(&first, "gone"),
        "the same line for a folder inside it: {said}"
    );
    assert!(lines_of(&said, "error").is_empty(), "{said}");
    assert!(l.request(&list(&l.dir.join("parent"))) == Control::Continue);
    let said = l.said();
    assert_eq!(
        field_str(said.lines().next().unwrap_or(""), "t").as_deref(),
        Some("listed"),
        "the folder it was sent to still lands: {said}"
    );
}

// A list sent inside the folder that went before the client read the line, going up into it or into a new folder made there, never lands.
#[test]
fn a_list_inside_the_folder_that_went_answers_the_line_again_even_where_a_new_folder_answers() {
    let mut l = Loop::new("rungate-gone-inside");
    let open = l.dir.dir("top/mid/open-me");
    open_listed(&mut l, &open);
    std::fs::rename(l.dir.join("top/mid"), l.dir.join("moved"))
        .expect("the sandbox moves its own folder");
    let first = said_until(&mut l, "gone", EVENT_DEADLINE);
    assert_eq!(lines_of(&first, "gone").len(), 1, "{first}");
    assert!(l.request(&list(&l.dir.join("top/mid"))) == Control::Continue);
    let said = l.said();
    assert_eq!(
        lines_of(&said, "gone"),
        lines_of(&first, "gone"),
        "going up into the folder that went: {said}"
    );
    assert!(lines_of(&said, "error").is_empty(), "{said}");
    let (again, child) = (l.dir.dir("top/mid/again"), l.dir.dir("top/mid/again/child"));
    let expanded = format!(
        r#"{{"c":"list","path":"{}","first":10,"outlinePaths":["{}"]}}"#,
        crate::json::escape(&text(&again)),
        crate::json::escape(&text(&child))
    );
    assert!(l.request(&expanded) == Control::Continue);
    let said = l.said();
    assert_eq!(
        lines_of(&said, "gone"),
        lines_of(&first, "gone"),
        "a new folder made there: {said}"
    );
    assert!(lines_of(&said, "listed").is_empty(), "{said}");
    let fd = l.watch.raw_fd();
    assert_eq!(watches_on(fd, &again), 0, "the scan's own watch goes");
    assert_eq!(watches_on(fd, &child), 0, "the expanded child's watch goes");
    assert!(l.request(&list(&l.dir.join("top"))) == Control::Continue);
    let said = l.said();
    assert_eq!(
        field_str(said.lines().next().unwrap_or(""), "t").as_deref(),
        Some("listed"),
        "the folder the line named still lands: {said}"
    );
}

// A folder another program merely writes into keeps answering changed, and a folder nobody had open answers its own error.
#[test]
fn a_change_inside_and_a_missing_folder_never_open_answer_as_before() {
    let mut l = Loop::new("rungate-gone-control");
    let open = l.dir.dir("open-me");
    open_listed(&mut l, &open);
    l.dir.file("open-me/new.txt", "n");
    let said = said_until(&mut l, "changed", EVENT_DEADLINE);
    assert_eq!(lines_of(&said, "changed").len(), 1, "{said}");
    assert!(lines_of(&said, "gone").is_empty(), "{said}");
    assert!(l.request(&list(&l.dir.join("never-there"))) == Control::Continue);
    let said = l.said();
    assert_eq!(lines_of(&said, "error").len(), 1, "{said}");
    assert!(lines_of(&said, "gone").is_empty(), "{said}");
}
