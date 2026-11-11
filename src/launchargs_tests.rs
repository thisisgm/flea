use super::Options;

fn parse(args: &[&str]) -> Result<Options, String> {
    let argv: Vec<String> = std::iter::once("flea")
        .chain(args.iter().copied())
        .map(String::from)
        .collect();
    Options::parse(&argv)
}

#[test]
fn help_is_asked_for_by_either_spelling_and_never_opens_a_folder() {
    for args in [
        &["--help"][..],
        &["-h"],
        &["--tui", "-h"],
        &["--tui", "--help", "/work"],
    ] {
        let options = parse(args).unwrap_or_else(|error| panic!("{args:?}: {error}"));
        assert!(options.help, "{args:?} asks for help");
    }
    assert!(!parse(&["/work"]).unwrap().help);
}

#[test]
fn a_dash_led_argument_is_a_flag_and_only_after_double_dash_a_path() {
    for flag in ["-x", "-V", "-", "--nonsense"] {
        let error = parse(&["--tui", flag])
            .err()
            .unwrap_or_else(|| panic!("{flag} was read as a start folder"));
        assert_eq!(error, format!("unknown flag {flag}"));
    }
    let options = parse(&["--tui", "--", "-h"]).unwrap();
    assert_eq!(
        options.start.as_deref(),
        Some("-h"),
        "after -- a folder named -h opens"
    );
    assert!(options.want_tui && !options.help);
    assert_eq!(
        parse(&["--tui", "--", "-V"]).unwrap().start.as_deref(),
        Some("-V")
    );
    assert_eq!(
        parse(&["--", "--tui"]).unwrap().start.as_deref(),
        Some("--tui")
    );
    assert!(!parse(&["--", "--tui"]).unwrap().want_tui);
    // A plain word is still a path, as a folder named help or flash must open.
    assert_eq!(parse(&["help"]).unwrap().start.as_deref(), Some("help"));
}
