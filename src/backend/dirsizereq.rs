// The dirsize queue and its one-at-a-time walker, kept beside the loop rather than inside it.
use crate::backend::dirsize;
use crate::backend::run::since;
use crate::backend::state::State;
use std::io::{self, BufWriter, Write};
use std::time::Instant;

// partial is true when the 2000 ms deadline cut the walk short, see docs/protocol.md "dirsized".
pub fn dirsized_line(row: usize, bytes: u64, partial: bool, ms: f64) -> String {
    format!(r#"{{"t":"dirsized","row":{},"bytes":{},"partial":{},"ms":{:.3}}}"#, row, bytes, partial, ms)
}

// Answered rows are re-answered at once, matching thumb's own cache-hit shape; only a directory can be asked for.
pub fn queue_dirsizes(out: &mut BufWriter<io::Stdout>, st: &mut State, rows: &[usize]) {
    for &row in rows {
        if row >= st.listing.len() || !st.listing.is_dir(row) {
            continue;
        }
        if let Some(&(bytes, partial)) = st.dirsizes.get(&row) {
            writeln!(out, "{}", dirsized_line(row, bytes, partial, 0.0)).ok();
            continue;
        }
        if st.dirsize_queue.contains(&row) {
            continue;
        }
        st.dirsize_queue.push(row);
    }
    out.flush().ok();
}

// One directory per call: no thread pool, so this is the whole of "one at a time" from AGENTS.md.
pub fn walk_one_dirsize(out: &mut BufWriter<io::Stdout>, st: &mut State) {
    let row = st.dirsize_queue.remove(0);
    if row >= st.listing.len() {
        return;
    }
    let path = st.base.join(st.listing.name(row));
    let t = Instant::now();
    let result = dirsize::walk(&path);
    let ms = since(t);
    st.dirsizes.insert(row, (result.bytes, result.partial));
    writeln!(out, "{}", dirsized_line(row, result.bytes, result.partial, ms)).ok();
    out.flush().ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_a_dirsized_line_complete_and_partial() {
        assert_eq!(
            dirsized_line(4, 1048576, false, 12.5),
            r#"{"t":"dirsized","row":4,"bytes":1048576,"partial":false,"ms":12.500}"#
        );
        // partial:true is a floor, not a wrong exact number; the cell renders it with a leading ">".
        assert_eq!(
            dirsized_line(9, 200, true, 2000.0),
            r#"{"t":"dirsized","row":9,"bytes":200,"partial":true,"ms":2000.000}"#
        );
    }
}

