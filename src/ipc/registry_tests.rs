use super::*;
use std::collections::BTreeSet;

#[test]
fn registry_matches_every_target_signature_in_the_frozen_oracle() {
    let mut expected = BTreeSet::new();
    for (target, text) in [
        ("flea", include_str!("../../ui/Ipc.qml")),
        ("fleapicker", include_str!("../../ui/PickerWindow.qml")),
    ] {
        let text = text.split_once(&format!("target: \"{target}\"")).unwrap().1;
        for line in text
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("function "))
        {
            let Some((head, result)) = line.split_once("): ") else {
                continue;
            };
            let (name, parameters) = head
                .trim_start_matches("function ")
                .split_once('(')
                .unwrap();
            let result = result.split_whitespace().next().unwrap();
            let parameters = parameters
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.split_once(": ").unwrap())
                .collect::<Vec<_>>();
            let registered = ORACLE
                .iter()
                .find(|f| f.target == target && f.name == name)
                .unwrap();
            assert_eq!(registered.parameters, parameters, "{target}.{name}");
            assert_eq!(registered.result, result, "{target}.{name}");
            assert!(expected.insert((target, name)), "duplicate {target}.{name}");
        }
    }
    assert_eq!(ORACLE.len(), 324);
    assert_eq!(expected.len(), ORACLE.len());
    assert_eq!(
        ORACLE
            .iter()
            .map(|f| (f.target, f.name))
            .collect::<BTreeSet<_>>(),
        expected
    );
}

#[test]
fn every_registered_function_has_a_distinct_not_ported_answer() {
    let mut messages = BTreeSet::new();
    for function in ORACLE {
        let reply = refused(function);
        assert_eq!(reply.code, 3);
        assert!(reply.stdout.is_empty());
        assert!(messages.insert(reply.stderr));
    }
    assert_eq!(messages.len(), 324);
}

#[test]
fn lookup_arity_and_argument_errors_match_quickshell() {
    assert_eq!(
        validate("missing", "path", &[]),
        Err(Reply::value("Target not found."))
    );
    assert_eq!(
        validate("flea", "missing", &[]),
        Err(Reply::value("Function not found."))
    );
    assert_eq!(validate("flea", "rowAt", &[]), Err(Reply::value(
        "Too few arguments provided (1 required but 0 were provided.)\nFunction definition: function rowAt(i: int): string")));
    assert_eq!(validate("flea", "rowAt", &["x".into()]), Err(Reply::value(
        "Unable to parse argument 1 as \"int\". Provided argument: \"x\"\nFunction definition: function rowAt(i: int): string")));
    assert_eq!(validate("flea", "rowAt", &["1".into(), "2".into()]), Err(Reply::value(
        "Too many arguments provided (1 required but 2 were provided.)\nFunction definition: function rowAt(i: int): string")));
    for value in ["true", "false", "1", "0"] {
        assert!(validate("flea", "htmlState", &[value.into()]).is_ok());
    }
    assert!(validate("flea", "rowAt", &["  +12  ".into()]).is_ok());
}
