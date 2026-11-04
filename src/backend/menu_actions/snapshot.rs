//! The snapshot reply carries captured kinds and modes from one inspection.
use super::*;

// Past this many items the answer carries no names, so a huge selection stays one short line.
const MAX_NAMED_ITEMS: usize = 512;

impl Snapshot {
    // Sample input: {"c":"menuaction","op":"snapshot","id":3}; run.rs resolves explicit paths or listing rows.
    pub(super) fn handle_request(
        &mut self,
        line: &str,
        paths: Vec<String>,
        cursor: Option<&str>,
        registry: &Registry,
        cancel: &Cancellation,
    ) -> String {
        let id = field_usize(line, "id").unwrap_or(0);
        let op = field_str(line, "op").unwrap_or_default();
        let result = if op == "snapshot" {
            self.id = 0;
            self.items.clear();
            self.cursor = None;
            self.deletion = None;
            self.provider_destination = None;
            self.provider_transfer = false;
            if id == 0 || paths.is_empty() {
                Err("There are no selected items to inspect.".into())
            } else {
                paths
                    .iter()
                    .map(|path| Selected::inspect(path))
                    .collect::<Result<Vec<_>, _>>()
                    .and_then(|items| {
                        self.cursor = cursor.map(Selected::inspect).transpose()?;
                        self.items = items;
                        self.id = id;
                        let kinds = self
                            .items
                            .iter()
                            .map(|item| item.kind.to_string())
                            .collect::<Vec<_>>()
                            .join(",");
                        let modes = self
                            .items
                            .iter()
                            .map(|item| item.mode.to_string())
                            .collect::<Vec<_>>()
                            .join(",");
                        let names = crate::json::names_field(
                            self.items.iter().map(|item| item.path.as_path()),
                            MAX_NAMED_ITEMS,
                        );
                        Ok(format!(
                            r#""count":{},"kinds":[{}],"modes":[{}]{names}"#,
                            self.items.len(),
                            kinds,
                            modes
                        ))
                    })
            }
        } else if op == "close" {
            if self.id == id {
                *self = Self::default();
            }
            Ok(String::new())
        } else {
            self.perform(id, &op, line, registry, cancel)
        };
        response(line, result)
    }
}
