use super::key;

type Version = (u8, u8, u8);

struct Probe {
    name: &'static str,
    observe: fn() -> String,
    expected: [&'static str; 2],
}

const PROBES: [Probe; 9] = [
    Probe {
        name: "Flea folded-name collision U+A7DD/U+0277",
        observe: || (key("\u{A7DD}.txt", true) == key("\u{277}.txt", true)).to_string(),
        expected: ["false", "true"],
    },
    Probe {
        name: "U+0277 uppercase",
        observe: || '\u{277}'.to_uppercase().collect(),
        expected: ["\u{277}", "\u{A7DD}"],
    },
    Probe {
        name: "U+AB4B uppercase",
        observe: || '\u{AB4B}'.to_uppercase().collect(),
        expected: ["\u{AB4B}", "\u{AB6C}"],
    },
    Probe {
        name: "Flea key U+A7DD lowercase",
        observe: || key("\u{A7DD}", true),
        expected: ["\u{A7DD}", "\u{277}"],
    },
    Probe {
        name: "U+1DF95 uppercase expansion",
        observe: || '\u{1DF95}'.to_uppercase().collect(),
        expected: ["\u{1DF95}", "SS"],
    },
    Probe {
        name: "U+A7DD uppercase property",
        observe: || '\u{A7DD}'.is_uppercase().to_string(),
        expected: ["false", "true"],
    },
    Probe {
        name: "U+12550 numeric property",
        observe: || '\u{12550}'.is_numeric().to_string(),
        expected: ["false", "true"],
    },
    Probe {
        name: "U+3D000 alphabetic property",
        observe: || '\u{3D000}'.is_alphabetic().to_string(),
        expected: ["false", "true"],
    },
    Probe {
        name: "U+1FA8B debug spelling",
        observe: || format!("{:?}", "\u{1FA8B}"),
        expected: ["\"\\u{1fa8b}\"", "\"\u{1FA8B}\""],
    },
];

fn version_row(version: Version) -> Option<usize> {
    match version {
        (17, 0, 0) => Some(0),
        (18, 0, 0) => Some(1),
        _ => None,
    }
}

const STABLE_FACTS: usize = 5;

fn stable_facts() -> usize {
    let facts: [(String, &str); STABLE_FACTS] = [
        (key("ASCII.TXT", true), "ascii.txt"),
        (key("ASCII.TXT", false), "ASCII.TXT"),
        (
            key("\u{C0}\u{C4}\u{D6}\u{DC}\u{C6}\u{D8}\u{DF}.TXT", true),
            "\u{E0}\u{E4}\u{F6}\u{FC}\u{E6}\u{F8}\u{DF}.txt",
        ),
        ('\u{E9}'.to_uppercase().collect(), "\u{C9}"),
        ('\u{DF}'.to_uppercase().collect(), "SS"),
    ];
    let mut checked = 0;
    for (seen, expected) in &facts {
        assert_eq!(seen, expected);
        checked += 1;
    }
    checked
}

/// The number of checks that ran: the stable facts, plus the nine probes when the version has a row.
fn validate(version: Version) -> usize {
    let mut checked = stable_facts();
    let Some(row) = version_row(version) else {
        return checked;
    };
    for probe in PROBES {
        assert_eq!(
            (probe.observe)(),
            probe.expected[row],
            "{} with Unicode {version:?}",
            probe.name
        );
        checked += 1;
    }
    checked
}

fn require_known(version: Version) {
    assert!(version_row(version).is_some(), "unknown Unicode table version {version:?}; refresh the nine casefold probes before changing the CI table");
}

#[test]
fn unicode_tables_match_the_compiler_version() {
    let version = std::char::UNICODE_VERSION;
    let checked = validate(version);
    if version_row(version).is_some() {
        assert_eq!(checked, STABLE_FACTS + PROBES.len());
        println!("casefold Unicode {version:?}: checked nine golden probes");
    } else {
        assert_eq!(checked, STABLE_FACTS);
        println!(
            "casefold Unicode {version:?}: unknown table, checked ASCII and Latin-1 folds only"
        );
    }
}

#[test]
fn a_future_version_checks_only_stable_folds() {
    assert_eq!(validate((255, 0, 0)), STABLE_FACTS);
}

#[test]
#[should_panic(expected = "unknown Unicode table version")]
fn the_strict_policy_rejects_a_future_version() {
    require_known((255, 0, 0));
}

#[test]
#[ignore = "tests/unicode-tables.sh runs the flea-ci-only strict version check"]
fn unicode_version_is_known() {
    require_known(std::char::UNICODE_VERSION);
}
