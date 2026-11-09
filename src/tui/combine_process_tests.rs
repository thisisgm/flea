//! Private process helpers isolate the real backend and image tools from operator state.
use super::convert_process_tests::tool;
use super::{
    combine,
    combine_tests::*,
    graphics::{Graphics, Protocol},
    model::Model,
    wire::Wire,
};
use crate::{backend::testdir::TestDir, jsondoc::Json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn scenario(case: &str) {
    if case == "review-navigation" {
        let identity = Command::new("/usr/bin/timeout")
            .arg("10")
            .arg(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
            .arg("-vV")
            .output()
            .unwrap();
        assert!(identity.status.success());
        eprintln!(
            "TUI11 review compiler: {}",
            String::from_utf8_lossy(&identity.stdout)
        );
    }
    let dir = TestDir::new("tui-combine-real");
    for name in [
        "home", "config", "cache", "data", "state", "runtime", "files", "tmp", "bin", "i18n/en",
    ] {
        dir.dir(name);
    }
    dir.file("i18n/en/flea.json", r#"{"tui.combine.running":"Combining images…","tui.combine.cancelled":"Image combination cancelled."}"#);
    if case == "font-fallback" {
        use std::os::unix::fs::PermissionsExt;
        dir.file(
            "bin/fc-match",
            "#!/bin/sh\nprintf x >> \"$FLEA_TUI_COMBINE_ROOT/font-lookups\"\nexit 1\n",
        );
        std::fs::set_permissions(
            dir.join("bin/fc-match"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let mut cmd = Command::new("/usr/bin/setsid");
    cmd.args([
        "-w",
        "/usr/bin/bwrap",
        "--unshare-user",
        "--unshare-pid",
        "--unshare-net",
        "--die-with-parent",
        "--ro-bind",
        "/",
        "/",
        "--proc",
        "/proc",
        "--dev-bind",
        "/dev",
        "/dev",
        "--bind",
    ])
    .arg(dir.path())
    .arg(dir.path())
    .args([
        "/bin/sh",
        "-c",
        "umask \"$1\"; shift; exec \"$@\"",
        "tui-combine-clean",
        if case == "privacy" { "000" } else { "022" },
    ])
    .arg(std::env::current_exe().unwrap())
    .args([
        "--exact",
        "tui::combine_process_tests::tui_combine_process_helper",
        "--ignored",
        "--nocapture",
    ])
    .env_clear()
    .env(
        "PATH",
        format!("{}:/usr/local/bin:/usr/bin:/bin", dir.join("bin").display()),
    )
    .env("FLEA_TUI_COMBINE_ROOT", dir.path())
    .env("FLEA_TUI_COMBINE_CASE", case)
    .env("FLEA_I18N", dir.join("i18n"))
    .env("TMPDIR", dir.join("tmp"))
    .env("MAGICK_THREAD_LIMIT", "1")
    .env("OMP_NUM_THREADS", "1")
    .env("LC_ALL", "C.UTF-8")
    .stdin(Stdio::null());
    for (key, name) in [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_RUNTIME_DIR", "runtime"),
    ] {
        cmd.env(key, dir.join(name));
    }
    assert!(
        cmd.status().unwrap().success(),
        "real combine scenario {case}"
    );
    assert_eq!(std::fs::read_to_string(dir.join("executed")).unwrap(), case);
    if case == "font-fallback" {
        assert_eq!(
            std::fs::read_to_string(dir.join("font-lookups")).unwrap(),
            "x"
        );
    }
}
fn backend() -> Wire {
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", r#""$1" --exact tui::combine_process_tests::tui_combine_backend_helper --ignored --nocapture --quiet | /usr/bin/sed -u -n '/^{/p'"#, "tui-combine-backend"])
        .arg(std::env::current_exe().unwrap()).env("FLEA_TUI_COMBINE_BACKEND", "1");
    Wire::spawn(cmd).unwrap()
}
#[test]
#[ignore = "private backend entry point"]
fn tui_combine_backend_helper() {
    if std::env::var_os("FLEA_TUI_COMBINE_BACKEND").is_some() {
        std::process::exit(crate::backend::run::run());
    }
}
#[test]
#[ignore = "private clean combination scenarios"]
fn tui_combine_process_helper() {
    let root = PathBuf::from(std::env::var_os("FLEA_TUI_COMBINE_ROOT").unwrap());
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    match std::env::var("FLEA_TUI_COMBINE_CASE").unwrap().as_str() {
        "order" => order(&root),
        "dimensions" => dimensions(&root),
        "graphics" => graphics_tests::run(&root),
        "font-fallback" => graphics_tests::run(&root),
        "jpeg" => jpeg(&root),
        "safe" => safety::run(&root),
        "privacy" => privacy::run(&root),
        "cache-revision" => cache::stale_cache_revision(&root),
        "review-navigation" => review::navigation(&root),
        "review-pointer" => review::pointer_targets(&root),
        "review-short" => review::short_terminal(&root),
        "review-columns" => review::oversized_columns(&root),
        "review-revision" => review::source_revision(&root),
        _ => panic!("unknown case"),
    }
    std::fs::write(
        root.join("executed"),
        std::env::var("FLEA_TUI_COMBINE_CASE").unwrap(),
    )
    .unwrap();
}
pub fn until(m: &mut Model, w: &mut Wire, done: impl Fn(&Model) -> bool) {
    let end = Instant::now() + Duration::from_secs(45);
    while !done(m) {
        combine::poll(m);
        match w.events.recv_timeout(Duration::from_millis(10)) {
            Ok(event) => m.receive(event.unwrap(), w).unwrap(),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                assert!(Instant::now() < end, "combine deadline: {}", m.error)
            }
            Err(error) => panic!("backend: {error}"),
        }
    }
}
fn fixture(root: &Path) -> Vec<PathBuf> {
    [
        ("a.png", "100x60", "red"),
        ("b.png", "40x80", "blue"),
        ("c.png", "60x40", "green"),
    ]
    .into_iter()
    .map(|(name, size, color)| {
        let path = root.join("files").join(name);
        tool(&[
            "magick",
            "-size",
            size,
            &format!("xc:{color}"),
            path.to_str().unwrap(),
        ]);
        path
    })
    .collect()
}
fn open(root: &Path) -> (Model, Wire) {
    let mut w = backend();
    let mut m = Model::new(root.join("files"), &Json::Null);
    m.columns = 130;
    m.height = 27;
    m.preview_state.cell = Some((8, 16));
    m.open(root.join("files"), &mut w).unwrap();
    until(&mut m, &mut w, |m| {
        m.pending.is_none() && m.rows.len() == m.total && m.total >= 3
    });
    m.selected = m
        .rows
        .iter()
        .filter(|(_, r)| ["a.png", "b.png", "c.png"].contains(&r.name.as_str()))
        .map(|(i, _)| *i)
        .collect();
    m.cursor = *m.selected.first().unwrap();
    m.remember_selection();
    super::actions::act(&mut m, "menu", &mut w).unwrap();
    until(&mut m, &mut w, |m| m.menu_ready);
    m.menu_cursor = super::render::menu_layout::entries(&m, &super::keymap::Map::load())
        .iter()
        .position(|row| row.as_ref().is_some_and(|r| r.id == "combine"))
        .expect("Combine row");
    press(&mut m, &mut w, super::input::Key::named("Return", ""));
    until(&mut m, &mut w, |m| {
        m.menu_action == "combineCard" || !m.error.is_empty()
    });
    assert!(m.error.is_empty(), "{}", m.error);
    assert_eq!(m.combining.card.as_ref().unwrap().paths.len(), 3);
    (m, w)
}
fn preview(m: &mut Model, w: &mut Wire, protocol: Protocol) -> Graphics {
    m.capabilities.graphics = protocol;
    let mut graphics = Graphics::new();
    graphics.protocol = protocol;
    let mut shown = super::stage::Shown::default();
    let mut frame = String::new();
    let end = Instant::now() + Duration::from_secs(45);
    loop {
        super::stage::step(m, &mut graphics, &mut shown, w, (130, 31), true, &mut frame).unwrap();
        let f = m.combining.card.as_ref().unwrap();
        assert!(f.error.is_empty(), "{}", f.error);
        if f.sizes.len() == 3 && (protocol == Protocol::None || !graphics.bytes.is_empty()) {
            return graphics;
        }
        assert!(Instant::now() < end, "preview deadline");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn submit(m: &mut Model, w: &mut Wire) {
    m.combining.card.as_mut().unwrap().focus = 8;
    press(m, w, super::input::Key::named("Return", ""));
    assert!(m.combining.run.is_some(), "real Combine submission");
    until(m, w, |m| m.combining.run.is_none());
}
fn identify(path: &Path, format: &str) -> String {
    String::from_utf8(tool(&[
        "magick",
        "identify",
        "-format",
        format,
        path.to_str().unwrap(),
    ]))
    .unwrap()
}
fn order(root: &Path) {
    fixture(root);
    let (mut m, mut w) = open(root);
    preview(&mut m, &mut w, Protocol::None);
    press(&mut m, &mut w, super::input::Key::character('l', ""));
    press(&mut m, &mut w, super::input::Key::character('[', ""));
    assert!(m.combining.card.as_ref().unwrap().paths[0].ends_with("b.png"));
    assert_eq!(m.combining.card.as_ref().unwrap().sizes[0], (40, 80));
    let g = combine::geometry(&m);
    let target = g.tiles[2];
    pointer(&mut m, &mut w, target.x + 1, target.y + 1, false, false);
    assert_eq!(m.combining.card.as_ref().unwrap().image, 2);
    let first = g.tiles[0];
    pointer(&mut m, &mut w, first.x + 1, first.y + 1, true, false);
    pointer(&mut m, &mut w, first.x + 1, first.y + 1, false, true);
    assert!(m.combining.card.as_ref().unwrap().paths[0].ends_with("c.png"));
    assert_eq!(m.combining.card.as_ref().unwrap().sizes[0], (60, 40));
    let dimensions = m.combining.card.as_ref().unwrap().composition().unwrap();
    submit(&mut m, &mut w);
    assert!(m.error.is_empty(), "{}", m.error);
    let path = root.join("files/c (combined).png");
    assert_eq!(
        identify(&path, "%w %h"),
        format!("{} {}", dimensions.width as u32, dimensions.height as u32)
    );
    let pixel = identify(&path, "%[pixel:p{10,10}]");
    assert!(
        pixel.contains("0,128,0"),
        "ordered first source must be green: {pixel}"
    );
}
fn dimensions(root: &Path) {
    fixture(root);
    for layout in 0..3 {
        for spacing in 0..3 {
            let (mut m, mut w) = open(root);
            choose(&mut m, &mut w, 1, layout);
            choose(&mut m, &mut w, 4, spacing);
            preview(&mut m, &mut w, Protocol::None);
            let f = m.combining.card.as_mut().unwrap();
            f.name = format!("result-{layout}-{spacing}.png");
            let c = f.composition().unwrap();
            submit(&mut m, &mut w);
            assert!(m.error.is_empty(), "{}", m.error);
            let path = root.join(format!("files/result-{layout}-{spacing}.png"));
            assert_eq!(
                identify(&path, "%m %w %h"),
                format!("PNG {} {}", c.width as u32, c.height as u32)
            );
            if spacing > 0 || layout == 2 {
                assert_eq!(identify(&path, "%[pixel:p{0,0}]"), "srgba(0,0,0,0)");
            }
        }
    }
}
fn jpeg(root: &Path) {
    fixture(root);
    let (mut m, mut w) = open(root);
    choose(&mut m, &mut w, 3, 1);
    let f = m.combining.card.as_ref().unwrap();
    assert_eq!(f.format, "jpg");
    assert_eq!(
        f.settings.background,
        flea::ui_logic_print_convert::composition::Background::White
    );
    assert!(f.name.ends_with(".jpg"));
    assert!(!combine::geometry(&m)
        .fields
        .iter()
        .any(|f| f.focus == 5 && f.text == "Transparent"));
    choose(&mut m, &mut w, 4, 0);
    assert!(combine::geometry(&m)
        .fields
        .iter()
        .filter(|f| f.focus == 2 || f.focus == 5)
        .all(|f| !f.enabled));
    choose(&mut m, &mut w, 1, 2);
    assert!(combine::geometry(&m)
        .fields
        .iter()
        .filter(|f| f.focus == 5)
        .all(|f| f.enabled));
    choose(&mut m, &mut w, 5, 1);
    submit(&mut m, &mut w);
    assert!(m.error.is_empty(), "{}", m.error);
    assert!(
        identify(&root.join("files/a (combined).jpg"), "%m %[opaque]").starts_with("JPEG True")
    );
}
#[path = "combine_safety_tests.rs"]
mod safety;

#[path = "combine_graphics_tests.rs"]
mod graphics_tests;
#[path = "combine_privacy_tests.rs"]
mod privacy;
#[path = "combine_review_tests.rs"]
mod review;

#[path = "combine_cache_tests.rs"]
mod cache;
