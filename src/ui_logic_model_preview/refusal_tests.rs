use super::wire::{self, Refusal, State};

#[test]
fn refusal_is_optional_and_unknown_categories_remain_generic() {
    for (extra, expected) in [
        ("", None),
        (r#", "refusal":"too_large""#, Some(Refusal::TooLarge)),
        (r#", "refusal":"future_code""#, None),
        (r#", "refusal":true"#, None),
    ] {
        let line = format!(
            r#"{{"t":"model3d","slot":"column","token":1,"state":"failed","reason":"XML structure exceeds its bound"{extra}}}"#
        );
        let State::Failed(failure) = wire::parse(&line).unwrap().state else {
            panic!("failed")
        };
        assert_eq!(failure.refusal, expected);
    }
}
