use super::*;

// Ported from tests/js/codelines.js; the rich-text escaping and font tags have no GPUI form, since a run paints as shaped glyphs.
#[test]
fn code_roles_are_bat_sgr_inks() {
    assert_eq!(role_name(0), "foreground", "role 0 is the foreground");
    let names: Vec<&str> = (1..=6).map(role_name).collect();
    assert_eq!(
        names.join(" "),
        "red green yellow blue magenta cyan",
        "roles 1 to 6 are bat's SGR 31 to 36"
    );
    assert_eq!(
        role_name(9),
        "foreground",
        "an unknown role is the foreground"
    );
    assert_eq!(
        role_name(-1),
        "foreground",
        "a negative role is the foreground"
    );
}

#[test]
fn code_tabs_fill_to_their_stop() {
    assert_eq!(
        plain(&[(0, "\tx")]),
        " ".repeat(8) + "x",
        "a tab at the line start fills to the next stop of eight"
    );
    assert_eq!(
        plain(&[(0, "abc\tx")]),
        "abc     x",
        "a tab after three characters fills five"
    );
    assert_eq!(
        pairs(&[(5, "ab"), (0, "\tx")]),
        vec![(5, "ab".to_string()), (0, "      x".to_string())],
        "a tab counts the columns of the runs before it"
    );
    assert_eq!(
        plain(&[(0, "a\tb")]),
        "a       b",
        "plain expands a tab to the next stop"
    );
}

#[test]
fn code_plain_joins_runs() {
    assert_eq!(
        plain(&[(5, "fn"), (0, " "), (4, "add")]),
        "fn add",
        "plain joins the runs' text"
    );
    assert_eq!(plain(&[]), "", "plain of nothing is empty");
    assert!(pairs(&[]).is_empty(), "an empty line is empty");
    let markup = "<img src=\"http://127.0.0.1:9/x\">";
    assert_eq!(
        plain(&[(2, markup)]),
        markup,
        "markup in a run stays its own text, never a tag"
    );
    assert_eq!(
        pairs(&[(2, "</font><b>x")])[0].0,
        2,
        "a run text cannot change its own role"
    );
}

// A glyph run keeps every space as shaped, so the rich-text space rules of codelines.js hold by construction.
#[test]
fn code_spaces_are_kept_as_written() {
    assert_eq!(plain(&[(0, "a  b")]), "a  b", "a repeated space is kept");
    assert_eq!(plain(&[(0, "  x")]), "  x", "a leading space is kept");
    assert_eq!(plain(&[(0, "x ")]), "x ", "a trailing space is kept");
    assert_eq!(plain(&[(0, " ")]), " ", "a space alone is kept");
    assert_eq!(
        plain(&[(5, "fn "), (0, " add")]),
        "fn  add",
        "two spaces split across runs are both kept"
    );
    let every = pairs(&[(1, "a"), (2, "b"), (3, "c"), (4, "d"), (5, "e"), (6, "f")]);
    let roles: Vec<i64> = every.iter().map(|(role, _)| *role).collect();
    assert_eq!(
        roles,
        vec![1, 2, 3, 4, 5, 6],
        "a run in every role keeps its own role"
    );
    assert_ne!(role_name(4), role_name(0), "blue is not the foreground");
}
