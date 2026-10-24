// Ported from tests/js/mdnest.js: the Source view's chunks are whole lines near the chunk size, none past the maximum, and together the text.
use super::*;

struct Chunked {
    count: usize,
    longest: usize,
    ends_in_newline: usize,
    same: bool,
}

fn chunked(text: &str) -> Chunked {
    let starts = source_chunk_starts(text);
    let mut again = String::new();
    let mut longest = 0;
    let mut ends_in_newline = 0;
    for i in 0..starts.len() {
        let piece = source_chunk(text, &starts, i);
        longest = longest.max(piece.len());
        if i + 1 < starts.len() && piece.ends_with('\n') {
            ends_in_newline += 1;
        }
        // A cut at a newline drops it from the piece and a cut inside a line drops nothing.
        let span = starts.get(i + 1).copied().unwrap_or(text.len()) - starts[i];
        again.push_str(piece);
        if piece.len() < span {
            again.push('\n');
        }
    }
    Chunked {
        count: starts.len(),
        longest,
        ends_in_newline,
        same: again == text,
    }
}

// n lines of width - 1 "w" each, every one ended by `ending`.
fn lines(n: usize, width: usize, ending: &str) -> String {
    format!("{}{ending}", "w".repeat(width - 1)).repeat(n)
}

#[test]
fn every_chunk_is_bounded_whole_lines_and_together_the_text() {
    let spaced = "word ".repeat(59_999);
    let corpus = [
        ("LF lines", lines(500, 100, "\n")),
        ("CRLF lines", lines(500, 100, "\r\n")),
        ("no final newline", lines(500, 100, "\n") + "tail"),
        ("a final newline", lines(500, 100, "\n")),
        ("blank lines", "\n".repeat(2999)),
        ("one 600 KB line without a newline", "w".repeat(600_000)),
        ("one 300 KB line of words", spaced.clone()),
        (
            "a long line between short ones",
            lines(80, 20, "\n") + &spaced + "\n" + &lines(80, 20, "\n"),
        ),
    ];
    for (name, text) in &corpus {
        let got = chunked(text);
        assert!(got.same, "{name}: the chunks are the text again");
        assert!(
            got.longest <= SOURCE_CHUNK_MAX,
            "{name}: no chunk is longer than the maximum"
        );
        assert_eq!(
            got.ends_in_newline, 0,
            "{name}: no chunk but the last keeps its newline"
        );
    }
    assert!(chunked(&corpus[5].1).count > 600_000 / SOURCE_CHUNK_MAX);
    let first = source_chunk(&spaced, &source_chunk_starts(&spaced), 0);
    assert!(first.ends_with(' '), "a line of words is cut after a space");
    assert_eq!(source_chunk_starts("a\nb\nc"), vec![0]);
    assert_eq!(
        source_chunk_starts(&lines(200, 100, "\n")),
        vec![0, 4100, 8200, 12300]
    );
}

#[test]
fn a_line_under_the_maximum_stays_whole_and_none_starts_on_a_newline() {
    let head = lines(80, 50, "\n");
    let long = "w".repeat(5000);
    let straddle = format!("{head}{long}\n{}", lines(80, 50, "\n"));
    let end = head.len() + long.len();
    let split: Vec<usize> = source_chunk_starts(&straddle)
        .into_iter()
        .filter(|start| *start > head.len() && *start <= end)
        .collect();
    assert!(
        split.is_empty(),
        "a 5000-byte line past short lines is kept whole"
    );
    let got = chunked(&straddle);
    assert!(got.same && got.longest <= SOURCE_CHUNK_MAX);
    let max_text = format!("{}\n{}", "w".repeat(SOURCE_CHUNK_MAX), lines(80, 50, "\n"));
    let on_newline = source_chunk_starts(&max_text)
        .into_iter()
        .filter(|start| max_text.as_bytes().get(*start) == Some(&b'\n'))
        .count();
    assert_eq!(
        on_newline, 0,
        "a max-length line leaves no chunk starting with a newline"
    );
    let got = chunked(&max_text);
    assert!(got.same && got.longest <= SOURCE_CHUNK_MAX);
}

#[test]
fn a_hard_cut_never_splits_a_character() {
    let run = format!("w{}", "\u{1F600}".repeat(5000));
    let starts = source_chunk_starts(&run);
    assert!(starts.len() > 1, "the run is cut");
    assert!(starts.iter().all(|start| run.is_char_boundary(*start)));
    let got = chunked(&run);
    assert!(got.same, "astral chunks are the text again");
    assert!(got.longest <= SOURCE_CHUNK_MAX, "astral chunks are bounded");
}
