// The user-dirs fixture cases from tests/js/userdirsfixture.js, split from places_tests.rs.
use super::userdirs::user_dirs;

const FIXTURE: [(&str, &str, Option<&str>); 27] = [
    (
        "escaped quote",
        r#""$HOME/Photo\"Inbox""#,
        Some(r#"/home/test/Photo"Inbox"#),
    ),
    (
        "escaped backslash",
        r#""$HOME/Photo\\Inbox""#,
        Some(r"/home/test/Photo\Inbox"),
    ),
    (
        "escaped dollar",
        r#""$HOME/Photo\$HOME""#,
        Some("/home/test/Photo$HOME"),
    ),
    (
        "escaped leading dollar",
        r#""/media/\$HOME/Inbox""#,
        Some("/media/$HOME/Inbox"),
    ),
    (
        "escaped backquote",
        r#""$HOME/Photo\`Inbox""#,
        Some("/home/test/Photo`Inbox"),
    ),
    (
        "leading HOME token",
        r#""$HOME/Inbox""#,
        Some("/home/test/Inbox"),
    ),
    ("absolute path", r#""/media/Inbox""#, Some("/media/Inbox")),
    (
        "surrounding whitespace",
        r#"  "$HOME/Inbox"  "#,
        Some("/home/test/Inbox"),
    ),
    (
        "trailing comment",
        r#""$HOME/Inbox" # local inbox"#,
        Some("/home/test/Inbox"),
    ),
    (
        "trailing spaces then comment",
        r#""$HOME/Inbox"   # local inbox"#,
        Some("/home/test/Inbox"),
    ),
    (
        "trailing quoted comment",
        r#""$HOME/Inbox" # "local inbox""#,
        Some("/home/test/Inbox"),
    ),
    (
        "literal dollar mid-path",
        r#""$HOME/Inbox$local""#,
        Some("/home/test/Inbox$local"),
    ),
    (
        "multiple trailing slashes",
        r#""$HOME/Inbox///" # local inbox"#,
        Some("/home/test/Inbox"),
    ),
    ("unterminated value", r#""$HOME/Inbox"#, None),
    ("dangling escape", r#""$HOME/Inbox\"#, None),
    (
        "concatenated quotes",
        r#""$HOME/Photo"Inbox""#,
        Some("/home/test/Photo"),
    ),
    (
        "trailing content",
        r#""$HOME/Inbox"junk"#,
        Some("/home/test/Inbox"),
    ),
    (
        "backslash makes any next character literal",
        r#""$HOME/Photo\q""#,
        Some("/home/test/Photoq"),
    ),
    (
        "unescaped dollar",
        r#""$HOME/Photo$HOME""#,
        Some("/home/test/Photo$HOME"),
    ),
    (
        "unescaped backquote",
        r#""$HOME/Photo`Inbox""#,
        Some("/home/test/Photo`Inbox"),
    ),
    (
        "HOME is literal after an absolute prefix",
        r#""/media/$HOME/Inbox""#,
        Some("/media/$HOME/Inbox"),
    ),
    ("HOME token has a suffix", r#""$HOMEwork/Inbox""#, None),
    ("relative path", r#""Inbox""#, None),
    (
        "escaped absolute prefix is relative",
        r#""\/media/Inbox""#,
        None,
    ),
    ("escaped HOME prefix is relative", r#""\$HOME/Inbox""#, None),
    ("single-quoted value", "'$HOME/Inbox'", None),
    ("unquoted value", "$HOME/Inbox", None),
];

fn fixture_body(value: &str) -> String {
    format!("XDG_DOWNLOAD_DIR={value}\nXDG_PICTURES_DIR=\"$HOME\"\nXDG_VIDEOS_DIR=\"$HOME\"")
}

fn paths(body: &str) -> Vec<String> {
    user_dirs(body, "/home/test")
        .into_iter()
        .map(|dir| dir.path)
        .collect()
}

#[test]
fn user_dirs_read_the_rail_roles() {
    let dirs = "XDG_DESKTOP_DIR=\"$HOME/\"\nXDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\nXDG_DOCUMENTS_DIR=\"$HOME/Documents\"\n# a comment line\nXDG_PROJECTS_DIR=\"/srv/projects\"\n";
    let got = user_dirs(dirs, "/home/gm");
    assert_eq!(got.len(), 3, "home itself is not a favourite");
    assert_eq!(
        got[0].path, "/home/gm/Downloads",
        "a relative entry expands"
    );
    assert_eq!(got[0].label, "Downloads", "the label is the leaf");
    assert_eq!(got[2].path, "/srv/projects", "an absolute entry survives");
    for (name, value, want) in FIXTURE {
        assert_eq!(
            paths(&fixture_body(value)),
            want.map(String::from).into_iter().collect::<Vec<_>>(),
            "rail user dirs: {name}"
        );
    }
    assert!(
        user_dirs(
            "XDG_PICTURES_DIR=\"$HOME\"\nXDG_VIDEOS_DIR=\"$HOME\"",
            "/home/test"
        )
        .is_empty(),
        "an absent role creates no conventional entry"
    );
}

#[test]
fn duplicate_roles_keep_the_last_valid_path() {
    for (name, value, want) in FIXTURE {
        let text = format!(
            "{}\nXDG_DOWNLOAD_DIR={value}",
            fixture_body(r#""$HOME/Original""#)
        );
        assert_eq!(
            paths(&text),
            vec![want.unwrap_or("/home/test/Original").to_owned()],
            "duplicate role rail: {name}"
        );
    }
    let bodies: [(String, Vec<&str>); 4] = [
        ("XDG_DOWNLOAD_DIR=\"relative\"\nXDG_PICTURES_DIR=\"$HOME/Photos\"\nXDG_DOWNLOAD_DIR=\"$HOME/Inbox\"\nXDG_VIDEOS_DIR=\"$HOME\"".into(), vec!["/home/test/Inbox", "/home/test/Photos"]),
        (format!("{}\nXDG_DOWNLOAD_DIR=\"$HOME\"\nXDG_DOWNLOAD_DIR=\"relative\"", fixture_body(r#""$HOME/Original""#)), vec![]),
        (format!("{}\nXDG_DOWNLOAD_DIR=\"$HOME/Inbox\"\nXDG_DOWNLOAD_DIR=\"$HOME/Unterminated", fixture_body(r#""$HOME""#)), vec!["/home/test/Inbox"]),
        ("XDG_PICTURES_DIR=\"$HOME/Old Photos\"\nXDG_DOWNLOAD_DIR=\"$HOME/Inbox\"\nXDG_PICTURES_DIR=\"$HOME/Photos\"\nXDG_VIDEOS_DIR=\"$HOME\"".into(), vec!["/home/test/Photos", "/home/test/Inbox"]),
    ];
    for (at, (body, want)) in bodies.iter().enumerate() {
        let rail = user_dirs(body, "/home/test");
        assert_eq!(
            rail.iter().map(|d| d.path.as_str()).collect::<Vec<_>>(),
            *want,
            "duplicate role rail order {at}"
        );
        let labels: Vec<&str> = want
            .iter()
            .map(|p| &p[p.rfind('/').unwrap() + 1..])
            .collect();
        assert_eq!(
            rail.iter().map(|d| d.label.as_str()).collect::<Vec<_>>(),
            labels,
            "duplicate role final labels {at}"
        );
    }
}
