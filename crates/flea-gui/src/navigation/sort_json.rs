//! DirSizes as the IPC reader writes it.
use super::DirSizes;

impl DirSizes {
    /// Pane.qml dirSizeState as JSON.stringify writes it: each asked row by index ascending (a JS object's integer keys), null while waiting, then the ask order.
    pub fn json(&self) -> String {
        let mut rows: Vec<usize> = self.order.iter().copied().collect();
        rows.sort_unstable();
        let file = rows
            .iter()
            .map(|row| {
                let answer = self.file.get(row).copied().flatten().map_or_else(
                    || "null".to_owned(),
                    |size| format!(r#"{{"bytes":{},"partial":{}}}"#, size.bytes, size.partial),
                );
                format!(r#""{row}":{answer}"#)
            })
            .collect::<Vec<_>>();
        let order = self.order.iter().map(usize::to_string).collect::<Vec<_>>();
        format!(
            r#"{{"file":{{{}}},"order":[{}]}}"#,
            file.join(","),
            order.join(",")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::DirSizes;

    // A scroll back up asks row 12 before row 3; JSON.stringify still writes the integer keys ascending.
    #[test]
    fn fixreaders_dir_size_state_writes_rows_ascending_after_a_backward_ask() {
        let mut sizes = DirSizes::default();
        sizes.applied(&[12, 3]);
        assert_eq!(
            sizes.json(),
            r#"{"file":{"3":null,"12":null},"order":[12,3]}"#
        );
    }
}
