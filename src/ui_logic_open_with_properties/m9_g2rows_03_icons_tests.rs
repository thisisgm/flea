use super::*;
struct TestDir(PathBuf);
impl TestDir {
    fn new(_tag: &str) -> Self {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/m9-g2rows-03");
        std::fs::create_dir_all(&base).unwrap();
        let made = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(base.join("icons.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(made.status.success());
        let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
        assert!(root.is_absolute() && root.starts_with(base));
        std::fs::write(root.join(".flea-test-sandbox"), "fixture\n").unwrap();
        Self(root)
    }
    fn dir(&self, name: &str) -> PathBuf {
        let p = self.0.join(name);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn file(&self, name: &str, text: &str) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, text).unwrap();
        p
    }
}

fn fixture() -> (TestDir, Icons) {
    let root = TestDir::new("m9-g2rows-03-icons");
    let data = root.dir("data");
    let home = root.dir("private-home");
    let lookup = Icons::from_data(
        "Fixture",
        Some(&home),
        Some(data.as_os_str()),
        Some(data.as_os_str()),
    );
    let theme = root.dir("data/icons/Fixture");
    std::fs::write(theme.join("index.theme"), "[Icon Theme]\nName=Fixture\nDirectories=18/apps,48/apps,scalable/apps\nScaledDirectories=18@2/apps\nInherits=Parent\n[18/apps]\nSize=18\nType=Fixed\n[48/apps]\nSize=48\nType=Fixed\n[scalable/apps]\nSize=24\nMinSize=16\nMaxSize=128\nType=Scalable\n[18@2/apps]\nSize=18\nScale=2\nType=Fixed\n").unwrap();
    for dir in ["18/apps", "48/apps", "scalable/apps", "18@2/apps"] {
        root.dir(&format!("data/icons/Fixture/{dir}"));
    }
    for name in ["Parent", "hicolor"] {
        let theme = root.dir(&format!("data/icons/{name}"));
        std::fs::write(
            theme.join("index.theme"),
            "[Icon Theme]\nDirectories=18/apps\nInherits=Fixture\n[18/apps]\nSize=18\nType=Fixed\n",
        )
        .unwrap();
        root.dir(&format!("data/icons/{name}/18/apps"));
    }
    (root, lookup)
}

#[test]
fn m9_g2rows_03_icons_named_absolute_missing_and_pixmaps() {
    let (root, lookup) = fixture();
    let named = root.file("data/icons/Fixture/18/apps/editor.svg", "<svg/>");
    let absolute = root.file("absolute.svg", "<svg/>");
    root.dir("data/pixmaps");
    let pixmap = root.file("data/pixmaps/legacy.png", "fixture");
    assert_eq!(lookup.resolve("editor", 18, 1), Some(named.clone()));
    assert_eq!(lookup.resolve("editor.svg", 18, 1), Some(named));
    assert_eq!(
        lookup.resolve(absolute.to_str().unwrap(), 18, 2),
        Some(absolute.clone())
    );
    assert_eq!(
        lookup.resolve(&format!("file://{}", absolute.display()), 18, 2),
        Some(absolute)
    );
    assert_eq!(lookup.resolve("legacy", 18, 1), Some(pixmap));
    assert_eq!(
        lookup.resolve("missing", 18, 1),
        None,
        "the renderer must retain app-window, never invent an executable icon"
    );
    assert_eq!(lookup.resolve("../editor", 18, 1), None);
    assert_eq!(lookup.resolve("/missing/absolute.svg", 18, 1), None);
}

#[test]
fn m9_g2rows_03_icons_size_scale_inheritance_and_hicolor() {
    let (root, lookup) = fixture();
    let small = root.file("data/icons/Fixture/18/apps/editor.svg", "small");
    let scaled = root.file("data/icons/Fixture/18@2/apps/editor.png", "scaled");
    let large = root.file("data/icons/Fixture/48/apps/editor.png", "large");
    let parent = root.file("data/icons/Parent/18/apps/inherited.svg", "parent");
    let fallback = root.file("data/icons/hicolor/18/apps/fallback.svg", "fallback");
    let vector = root.file("data/icons/Fixture/scalable/apps/vector.svg", "vector");
    assert_eq!(lookup.resolve("editor", 18, 1), Some(small));
    assert_eq!(lookup.resolve("editor", 18, 2), Some(scaled));
    assert_eq!(lookup.resolve("editor", 44, 1), Some(large));
    assert_eq!(lookup.resolve("inherited", 18, 1), Some(parent));
    assert_eq!(lookup.resolve("fallback", 18, 1), Some(fallback));
    assert_eq!(lookup.resolve("vector", 96, 2), Some(vector));
    assert_eq!(
        lookup.resolve("missing", 18, 1),
        None,
        "inheritance cycles terminate"
    );
}

#[test]
fn m9_g2rows_03_icons_private_home_keeps_system_data_dirs() {
    for dirs in [None, Some(std::ffi::OsStr::new(""))] {
        let lookup = Icons::from_data("Fixture", Some(Path::new("/private/home")), None, dirs);
        assert!(lookup.bases.contains(&PathBuf::from("/usr/share/icons")));
        assert!(lookup
            .pixmaps
            .contains(&PathBuf::from("/usr/share/pixmaps")));
    }
    let explicit = Icons::from_data(
        "Fixture",
        None,
        None,
        Some(std::ffi::OsStr::new("relative:/fixture/share")),
    );
    assert_eq!(explicit.bases, [PathBuf::from("/fixture/share/icons")]);
}

#[test]
fn m9_g2rows_03_icons_relative_data_home_uses_the_default() {
    let root = TestDir::new("relative-data-home");
    let home = root.dir("private-home");
    root.dir("private-home/.local/share/icons/Fixture/18/apps");
    root.file(
        "private-home/.local/share/icons/Fixture/index.theme",
        "[Icon Theme]\nDirectories=18/apps\n[18/apps]\nSize=18\nType=Fixed\n",
    );
    let icon = root.file(
        "private-home/.local/share/icons/Fixture/18/apps/editor.svg",
        "<svg/>",
    );
    let system = root.dir("system-data");
    let lookup = Icons::from_data(
        "Fixture",
        Some(&home),
        Some(std::ffi::OsStr::new("relative")),
        Some(system.as_os_str()),
    );
    assert_eq!(lookup.resolve("editor", 18, 1), Some(icon));
}

#[test]
fn m9_g2rows_03_icons_explicit_hicolor_keeps_parent_order() {
    let (root, lookup) = fixture();
    let index = root.0.join("data/icons/Fixture/index.theme");
    let text = std::fs::read_to_string(&index)
        .unwrap()
        .replace("Inherits=Parent", "Inherits=hicolor,Parent");
    std::fs::write(index, text).unwrap();
    let expected = root.file("data/icons/hicolor/18/apps/editor.svg", "<svg/>");
    root.file("data/icons/Parent/18/apps/editor.svg", "<svg/>");
    assert_eq!(lookup.resolve("editor", 18, 1), Some(expected));
}

#[test]
fn m9_g2rows_03_icons_blank_theme_uses_gtk() {
    const PROBE: &str = "FLEA_MENU_ROWS_ICON_THEME_PROBE";
    if let Some(root) = std::env::var_os(PROBE) {
        let lookup = Icons::session();
        assert_eq!(lookup.theme, "Fixture");
        assert_eq!(
            lookup.resolve("editor", 18, 1),
            Some(PathBuf::from(root).join("data/icons/Fixture/18/apps/editor.svg"))
        );
        return;
    }
    let (root, _) = fixture();
    let home = root.dir("private-home");
    root.dir("private-home/.local/state/omarchy/current/theme");
    root.file(
        "private-home/.local/state/omarchy/current/theme/icons.theme",
        "\n",
    );
    let config = root.dir("private-home/.config");
    root.dir("private-home/.config/gtk-3.0");
    root.file(
        "private-home/.config/gtk-3.0/settings.ini",
        "[Settings]\ngtk-icon-theme-name=Fixture\n",
    );
    let data = root.dir("data");
    root.file("data/icons/Fixture/18/apps/editor.svg", "<svg/>");
    for config_home in [config.as_os_str(), std::ffi::OsStr::new("relative")] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ui_logic_open_with_properties::openwith::icons::tests::m9_g2rows_03_icons_blank_theme_uses_gtk",
            ])
            .env(PROBE, &root.0)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", config_home)
            .env("XDG_DATA_HOME", &data)
            .env("XDG_DATA_DIRS", &data)
            .env_remove("QS_ICON_THEME")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"),
            "the child must execute the exact session probe"
        );
    }
}

#[test]
fn m9_g2rows_03_icons_system_pixmaps_survive_data_dir_override() {
    let lookup = Icons::from_data(
        "Fixture",
        Some(Path::new("/private/home")),
        None,
        Some(std::ffi::OsStr::new("/opt/share")),
    );
    assert!(lookup
        .pixmaps
        .contains(&PathBuf::from("/usr/share/pixmaps")));
}
