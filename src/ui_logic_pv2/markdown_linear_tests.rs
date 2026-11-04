// A blank run under a list item scans each newline run once, so the bytes the scan walks grow with the input,
// not its square: the review's "- parent" shape with k blank lines and k spaces before the last line.
fn blank_run(k: usize) -> String {
    format!("- parent\n{}{}x", "\n".repeat(k), " ".repeat(k))
}

// The newline-run bytes split_html_after_blank walks over the shape above.
fn walked_bytes(k: usize) -> usize {
    let text = blank_run(k);
    let mut walked = 0usize;
    std::hint::black_box(
        super::markdown_document::run::split_html_after_blank_counted(&text, &mut walked),
    );
    walked
}

#[test]
fn a_blank_run_scans_each_newline_run_once() {
    // Quadrupling a linear scan quadruples the bytes it walks; the uncached scan walked 16x.
    let small = walked_bytes(1024);
    let large = walked_bytes(4096);
    assert!(
        large < 8 * small,
        "quadrupling the blank run walked {large} bytes against {small}, which is quadratic"
    );
    // Absolute bound: each input byte is walked a constant number of times.
    let len = blank_run(4096).len();
    assert!(
        large <= 4 * len,
        "the scan walked {large} bytes over a {len} byte input"
    );
}
