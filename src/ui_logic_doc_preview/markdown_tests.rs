// Ported from tests/js/markdown.js: the stable API, links, pictures, counts and the block model.
use super::*;

fn ctx_words() -> Words {
    Words::english()
}

#[test]
fn line_count_reads_a_trailing_newline_as_the_end_of_a_line() {
    assert_eq!(line_count(""), 0);
    assert_eq!(line_count("a"), 1);
    assert_eq!(line_count("a\n"), 1);
    assert_eq!(line_count("a\nb"), 2);
    let words = ctx_words();
    let ctx = Ctx {
        dir: "/doc",
        chrome: "#181825",
        ink: "#c0caf5",
        words: &words,
    };
    let parsed = blocks("# Title\n\nSome *text*.\n", &ctx);
    assert_eq!(parsed[0].kind(), "heading");
}
