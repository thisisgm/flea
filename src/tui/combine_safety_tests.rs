//! Collision, source replacement and both asynchronous cancellation handoffs use private files.
use super::super::input::Key;
use super::*;
use std::os::unix::fs::PermissionsExt;

pub(super) fn run(root: &Path) {
    let paths = fixture(root);
    let before: Vec<_> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();
    let dest = root.join("files/a (combined).png");
    std::fs::write(&dest, b"collision sentinel").unwrap();
    let (mut m, mut w) = open(root);
    submit(&mut m, &mut w);
    assert!(m.error.contains("already exists"), "{}", m.error);
    assert_eq!(std::fs::read(&dest).unwrap(), b"collision sentinel");
    let (mut m, mut w) = open(root);
    m.combining.card.as_mut().unwrap().name = "../escape.png".into();
    press(&mut m, &mut w, Key::named("Return", ""));
    assert!(m.combining.run.is_none());
    assert!(m
        .combining
        .card
        .as_ref()
        .unwrap()
        .error
        .contains("file name"));
    press(&mut m, &mut w, Key::named("Escape", ""));
    let (mut m, mut w) = open(root);
    let id = m.action_id;
    m.refresh(&mut w).unwrap();
    until(&mut m, &mut w, |m| m.combining.card.is_none());
    m.receive(crate::jsondoc::parse(&format!(r#"{{"t":"menuaction","op":"validate","action":"combine","id":{id},"ok":true,"paths":["/old/a.png","/old/b.png"]}}"#)).unwrap(), &mut w).unwrap();
    assert!(m.combining.card.is_none());
    install_slow_sandbox(root);
    let (mut m, mut w) = open(root);
    let mut graphics = Graphics::new();
    let mut shown = super::super::stage::Shown::default();
    let mut frame = String::new();
    std::fs::write(root.join("slow"), b"owned slow preview").unwrap();
    super::super::stage::step(
        &mut m,
        &mut graphics,
        &mut shown,
        &mut w,
        (130, 31),
        true,
        &mut frame,
    )
    .unwrap();
    let pid = marker(root);
    press(&mut m, &mut w, Key::named("Escape", ""));
    let deadline = Instant::now() + Duration::from_secs(10);
    while combine::preview_pending(&m) {
        combine::poll(&mut m);
        assert!(Instant::now() < deadline, "preview task did not reap");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "cancel must reap decoder"
    );
    assert!(m.combining.card.is_none() && graphics.bytes.is_empty());
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::rename(root.join("held"), root.join("preview-reaped")).unwrap();
    epoch_change(root);
    let (mut m, mut w) = open(root);
    m.combining.card.as_mut().unwrap().name = "cancelled.png".into();
    press(&mut m, &mut w, Key::named("Return", ""));
    let pid = marker(root);
    until(&mut m, &mut w, |m| m.menu_action.is_empty());
    m.receive(
        crate::jsondoc::parse(
            r#"{"t":"combinedone","requestId":9999,"id":9999,"source":"/wrong","ok":true}"#,
        )
        .unwrap(),
        &mut w,
    )
    .unwrap();
    assert!(
        m.combining.run.is_some(),
        "stale completion cannot end active operation"
    );
    press(&mut m, &mut w, Key::named("Escape", ""));
    until(&mut m, &mut w, |m| m.combining.run.is_none());
    assert!(
        m.message.contains("Image combination cancelled."),
        "{} {}",
        m.message,
        m.error
    );
    assert!(!root.join("files/cancelled.png").exists());
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    for (path, bytes) in paths.iter().zip(before) {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}
pub(super) fn marker(root: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(text) = std::fs::read_to_string(root.join("held")) {
            return text.trim().parse().expect("complete PID marker");
        }
        assert!(Instant::now() < deadline, "sandbox did not start");
        std::thread::sleep(Duration::from_millis(5));
    }
}
pub(super) fn install_slow_sandbox(root: &Path) {
    let path = root.join("bin/bwrap");
    // Input sample: bwrap --die-with-parent --ro-bind /usr /usr ... magick -- /work/input-0.png.
    let script = r#"#!/bin/sh
set -eu
root=${FLEA_TUI_COMBINE_ROOT:?}
case "$root" in /*) test -f "$root/.flea-test-sandbox" ;; *) exit 98 ;; esac
if test -f "$root/slow"; then
  printf '%s\n' "$$" > "$root/held.part"
  mv "$root/held.part" "$root/held"
  exec /bin/sleep 60
fi
exec /usr/bin/bwrap "$@"
"#;
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn epoch_change(root: &Path) {
    let (mut m, mut w) = open(root);
    let mut graphics = Graphics::new();
    let mut shown = super::super::stage::Shown::default();
    let mut frame = String::new();
    super::super::stage::step(
        &mut m,
        &mut graphics,
        &mut shown,
        &mut w,
        (130, 31),
        true,
        &mut frame,
    )
    .unwrap();
    let pid = marker(root);
    press(&mut m, &mut w, Key::character('l', ""));
    press(&mut m, &mut w, Key::character('[', ""));
    super::super::stage::step(
        &mut m,
        &mut graphics,
        &mut shown,
        &mut w,
        (130, 31),
        true,
        &mut frame,
    )
    .unwrap();
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::rename(root.join("slow"), root.join("epoch-slow-used")).unwrap();
    preview(&mut m, &mut w, Protocol::None);
    let f = m.combining.card.as_ref().unwrap();
    assert!(f.paths[0].ends_with("b.png"));
    assert_eq!(
        f.sizes[0],
        (40, 80),
        "late preview cannot restore original source order"
    );
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    press(&mut m, &mut w, Key::named("Escape", ""));
    std::fs::rename(root.join("held"), root.join("epoch-reaped")).unwrap();
    std::fs::write(root.join("slow"), b"owned slow publication").unwrap();
}
