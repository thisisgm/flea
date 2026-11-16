use super::*;

// A test reading the repo's i18n/ sees every shipped catalog, so its English expectations break when a language lands.
#[test]
fn no_test_loads_the_shipped_catalog_dir() {
    const ALLOWED: &[(&str, &str)] = &[
        (
            "crates/flea-gui/src/window/rail_language_tests.rs",
            "Live rail language switching verifies the shipped XDG translations",
        ),
        (
            "crates/flea-gui/src/window/rail_language_every_tests.rs",
            "Every-language live rail switching reads each shipped catalog's place words",
        ),
        (
            "src/tui/live_language_tests.rs",
            "Live TUI language switching draws each shipped catalog's frame words",
        ),
        (
            "crates/flea-gui/src/g2/settings_pages/sheet_keycap_tests.rs",
            "I50-24 checks key sheet labels against each shipped catalog's keycap words",
        ),
        (
            "crates/flea-gui/src/row/fit_tests.rs",
            "I50-26 measures that the shipped translations fit their rows",
        ),
        (
            "crates/flea-gui/src/g2/settings_pages/rail_fit_tests.rs",
            "S3-5 measures every shipped catalog's Settings rail labels inside the rail",
        ),
        (
            "src/ui_logic_file_ops/tests/heads.rs",
            "S7C-5 builds running job heads from the shipped German catalog",
        ),
        (
            "src/ui_logic_trash_browser/words_tests.rs",
            "TR2-c formats the leftover footer with all 50 shipped catalogs",
        ),
        (
            "src/backend/kind_polish_tests.rs",
            "LANG polish verifies PNG fallback completeness and provider precedence in all 50 shipped catalogs",
        ),
        (
            "src/ui_logic_status_feedback/tests/unheld.rs",
            "FX-I18N3 verifies unsupported-entry reasons and copy/move plurals in every shipped catalog",
        ),
        (
            "crates/flea-gui/src/window/dispatch/lang_polish_tests.rs",
            "LANG polish measures actual German/French/Arabic catalog text in preview metadata",
        ),
        (
            "crates/flea-gui/src/window/dispatch/rename_refusal_tests.rs",
            "FX-I18N2 reads a drive name refusal under the rename draft in the shipped German catalog",
        ),
        (
            "src/i18n/locale.rs",
            "Catalog-only zero digits match Omarchy's roster for every shipped catalog",
        ),
    ];
    // Split so this file never matches itself; cargo runs tests from the package root, so cwd and relative forms name i18n/ too.
    let manifest = concat!("CARGO_MANIFEST_", "DIR\")");
    let cwd = concat!("current_dir", "().unwrap()");
    let mut banned = Vec::new();
    for tail in ["i18n", "../../i18n"] {
        banned.push(format!("{manifest}).join(\"{tail}\")"));
        banned.push(format!("{manifest},\"/{tail}\")"));
        banned.push(format!("{cwd}.join(\"{tail}\")"));
        banned.push(format!("Path::new(\"{tail}\")"));
        banned.push(format!("PathBuf::from(\"{tail}\")"));
    }
    let found: Vec<String> = rust_sources(&["src", "crates", "tests"])
        .into_iter()
        .filter(|(_, text)| banned.iter().any(|pattern| text.contains(pattern.as_str())))
        .map(|(path, _)| path)
        .collect();
    let unexpected: Vec<_> = found
        .iter()
        .filter(|path| !ALLOWED.iter().any(|(allowed, _)| allowed == path))
        .collect();
    assert!(
        unexpected.is_empty(),
        "load test_catalogs::english_only() or a fixture catalog instead of the repo's i18n/, or allow the file with a reason: {unexpected:?}"
    );
    let stale: Vec<_> = ALLOWED
        .iter()
        .map(|(path, _)| *path)
        .filter(|path| !found.iter().any(|seen| seen == path))
        .collect();
    assert!(
        stale.is_empty(),
        "allowed files no longer load the repo's i18n/: {stale:?}"
    );
}

// Sample output: ("src/plugincli.rs", "usecrate::i18n::I18n;..."), the repo-relative path and its whitespace-free text.
fn rust_sources(tops: &[&str]) -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut dirs: Vec<PathBuf> = tops.iter().map(|top| root.join(top)).collect();
    let mut found = Vec::new();
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() && path.file_name().is_some_and(|name| name != "target") {
                dirs.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text: String = fs::read_to_string(&path)
                    .unwrap()
                    .split_whitespace()
                    .collect();
                found.push((path.strip_prefix(root).unwrap().display().to_string(), text));
            }
        }
    }
    found.sort();
    found
}

// Production code resolves the host's language and catalogs; a test that did the same changes with the host and the shipped set.
#[test]
fn no_test_resolves_the_host_language() {
    // Each allowed file with its exact count, so a new test call inside it still fails.
    const ALLOWED: &[(&str, usize, &str)] = &[
        (
            "src/i18n/mod.rs",
            1,
            "I18n's Default impl is the real resolution",
        ),
        (
            "src/i18n/test_catalogs.rs",
            1,
            "the hermetic paths keep the default Omarchy root",
        ),
        (
            "src/plugincli.rs",
            1,
            "flea plugin prints in the user's language",
        ),
        (
            "src/backend/plugins.rs",
            1,
            "the plugin hello reports the user's language",
        ),
        (
            "src/backend/kind.rs",
            1,
            "the row service resolves the selected language once per response",
        ),
        (
            "src/backend/contents.rs",
            1,
            "backend content messages use the user's language",
        ),
        (
            "src/tui/model.rs",
            1,
            "Model::new's release build reads the user's language",
        ),
        (
            "src/tui/words.rs",
            1,
            "Text's Display implementation resolves the user's catalog only outside tests",
        ),
        (
            "crates/flea-gui/src/main.rs",
            1,
            "the GUI launch loads the real catalogs",
        ),
        (
            "crates/flea-gui/src/ipc/server.rs",
            1,
            "the IPC server answers in the user's language",
        ),
        (
            "crates/flea-gui/src/g2/media_preview/kit/window.rs",
            1,
            "the bench-seam media kit loads catalogs as the shipped app does",
        ),
    ];
    // Split so this file never matches itself; every spelling that names the host-resolving Default directly.
    let calls = [
        concat!("I18n::", "default"),
        concat!("Paths::", "default"),
        concat!("I18nas", "Default"),
        concat!("Pathsas", "Default"),
        concat!(":I18n=", "Default::default()"),
        concat!(":Paths=", "Default::default()"),
        concat!("i18n:", "Default::default()"),
    ];
    let mut unexpected = Vec::new();
    let mut seen = Vec::new();
    for (path, text) in rust_sources(&["src", "crates", "tests"]) {
        let count: usize = calls.iter().map(|call| text.matches(call).count()).sum();
        if count == 0 {
            continue;
        }
        match ALLOWED.iter().find(|(allowed, _, _)| *allowed == path) {
            Some((allowed, expected, _)) => {
                seen.push(*allowed);
                if count != *expected {
                    unexpected.push(format!("{path} ({count}, allowed {expected})"));
                }
            }
            None => unexpected.push(format!("{path} ({count})")),
        }
    }
    assert!(unexpected.is_empty(), "use test_catalogs::english() or english_paths(), or allow the file with a reason: {unexpected:?}");
    let stale: Vec<_> = ALLOWED
        .iter()
        .map(|(path, _, _)| *path)
        .filter(|path| !seen.contains(path))
        .collect();
    assert!(
        stale.is_empty(),
        "allowed files no longer call the default: {stale:?}"
    );
}
