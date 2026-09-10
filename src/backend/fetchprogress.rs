// The progress half of a fetch: what gio copy -p prints, and how often the wire hears about it.
use crate::backend::opsreq::PROGRESS_EVERY;
use std::io::Read;
use std::time::Instant;

// gio copy -p writes "Copied 1.2 MB out of 5.0 MB (1.1 MB/s; average: 1.4 MB/s)" behind a carriage
// return and an erase-line escape; older glib said "Transferred". The two figures are what the wire
// carries, rounded as gio rounded them; the rate is not. The final "Copied 5.0 MB (average: ...)"
// has no "out of" and is not progress.
pub fn parse_progress(text: &str) -> Option<(u64, u64)> {
    let (done, rest) = text.split_once(" out of ")?;
    let total = rest.split(" (").next()?;
    Some((size_at_end(done)?, size_at_end(total)?))
}

// The number and unit that end a phrase, in g_format_size's decimal units. The separator is a
// no-break space on this glib and a plain space on older ones, so both are word breaks here.
fn size_at_end(phrase: &str) -> Option<u64> {
    let mut words = phrase.split([' ', '\u{a0}']).filter(|w| !w.is_empty());
    let unit = words.next_back()?;
    let number: f64 = words.next_back()?.parse().ok()?;
    let scale: f64 = match unit {
        "byte" | "bytes" => 1.0,
        "kB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "PB" => 1e15,
        "EB" => 1e18,
        _ => return None,
    };
    Some((number * scale).round() as u64)
}

// Segments end at a carriage return as well as a newline, because every progress update overwrites
// the last one on a terminal. At most one report per PROGRESS_EVERY, the first at once.
pub fn read_progress(out: impl Read, mut report: impl FnMut(u64, u64)) {
    let mut out = out;
    let mut pending: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    let mut last: Option<Instant> = None;
    loop {
        let n = match out.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        pending.extend_from_slice(&buf[..n]);
        while let Some(end) = pending.iter().position(|b| *b == b'\r' || *b == b'\n') {
            let segment: Vec<u8> = pending.drain(..=end).collect();
            let Some((bytes, total)) = parse_progress(&String::from_utf8_lossy(&segment)) else {
                continue;
            };
            if !last.is_some_and(|t| t.elapsed() < PROGRESS_EVERY) {
                report(bytes, total);
                last = Some(Instant::now());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_progress_parser_reads_gio_own_line_and_nothing_else() {
        // glib 2.88 with LC_ALL=C, a no-break space before each unit, behind the terminal escape.
        assert_eq!(parse_progress("\r\u{1b}[KCopied 128.0\u{a0}kB out of 2.0\u{a0}MB (0 bytes/s; average: 8.0\u{a0}MB/s)"), Some((128000, 2000000)));
        assert_eq!(parse_progress("Transferred 1.2 MB out of 5.0 MB (300 kB/s)"), Some((1200000, 5000000)));
        // A server that sent no length: gio prints the unknown total as 0 bytes.
        assert_eq!(parse_progress("Copied 121 bytes out of 0 bytes (0 bytes/s; average: 650 bytes/s)"), Some((121, 0)));
        assert_eq!(parse_progress("Copied 1 byte out of 3.5\u{a0}GB (x)"), Some((1, 3_500_000_000)));
        assert_eq!(parse_progress("Copied 2.0\u{a0}TB out of 1.0\u{a0}PB (x)"), Some((2_000_000_000_000, 1_000_000_000_000_000)));
        assert_eq!(parse_progress("\u{1b}[KCopied 2.0\u{a0}MB (average: 1.4\u{a0}MB/s)\n"), None);
        assert_eq!(parse_progress("Copied 2.0 MiB out of 4.0 MiB (x)"), None, "binary units are not what LC_ALL=C gio prints");
        assert_eq!(parse_progress("gio: http://x/a: Not Found"), None);
        assert_eq!(parse_progress(""), None);
    }

    #[test]
    fn segments_split_on_carriage_return_and_newline_and_the_first_report_is_at_once() {
        let feed: &[u8] = b"\r\x1b[KCopied 1 byte out of 4 bytes (x)\r\x1b[KCopied 2 bytes out of 4 bytes (x)\nCopied 4 bytes (average: x)\n";
        let mut got = Vec::new();
        read_progress(feed, |b, t| got.push((b, t)));
        // The second update lands inside the 150 ms throttle, so only the first is reported.
        assert_eq!(got, vec![(1, 4)]);
    }
}
