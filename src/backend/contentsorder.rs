use super::*;
use std::collections::HashMap;

// Span offsets survive ordering, including two distinct names with the same lossy display.
pub fn sort(
    line: &str,
    anchor: Option<&str>,
    out: &mut impl Write,
    st: &mut State,
    tb: &Tables,
    pool: &Pool,
) {
    let Some(held) = st.contents.held.as_ref() else {
        return;
    };
    let by = field_str(line, "by").unwrap_or_default();
    if ["size", "mtime", "date"].contains(&by.as_str())
        && held.matches.iter().any(|hit| !hit.operable)
    {
        super::super::run::say(out, &identity_error("sort"));
        return;
    }
    let mut matches: HashMap<_, _> = st
        .listing
        .spans
        .iter()
        .zip(&held.matches)
        .map(|(span, hit)| (span.off, hit.clone()))
        .collect();
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let base = st.base.clone();
    let mut listing = st.listing.clone();
    let mime = std::sync::Arc::clone(&tb.mime);
    let request = line.to_owned();
    let sorted = super::super::iomount::call_bulk(&base.clone(), &body, "sort", move || {
        let result = super::super::ordering::request(&mut listing, &base, &mime, &request);
        result.map(|times| (listing, times)).map_err(str::to_owned)
    });
    let (listing, (pass_ms, sort_ms, _)) = match sorted {
        Ok(Ok(sorted)) => sorted,
        result => {
            let error = match result {
                Err(error) => error,
                Ok(Err(msg)) => FleaError {
                    where_: "sort".into(),
                    path: by,
                    msg,
                },
                _ => unreachable!(),
            };
            super::super::run::say(out, &super::super::proto::error_line(&error));
            return;
        }
    };
    let held = st.contents.held.as_mut().expect("contents sort snapshot");
    held.matches = listing
        .spans
        .iter()
        .enumerate()
        .map(|(row, span)| {
            let mut hit = matches.remove(&span.off).expect("sort preserves spans");
            hit.row = row;
            hit
        })
        .collect();
    let (gen, dev, writable, count) = (held.ask.gen, held.dev, held.writable, held.ask.count);
    st.listing = listing;
    forget_rows(st, pool);
    let mut listed = super::super::proto::listed_gen_line(
        st.listing.len(),
        pass_ms,
        sort_ms,
        dev,
        &st.base.to_string_lossy(),
        writable,
        gen,
    );
    if let Some(anchor) = anchor {
        listed = super::super::proto::with_anchor(
            &listed,
            anchor,
            st.listing
                .index_of(&st.base, Path::new(anchor))
                .map_or(-1, |row| row as isize),
        );
    }
    metadata(out, st, 0, count);
    super::super::run::say(out, &super::super::rowguard::stamped(listed, st.generation));
    rows(out, st, tb, 0, count);
}
