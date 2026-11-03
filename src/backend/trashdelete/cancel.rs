// Cancellable tree removal retains the ordinary claimed-payload rollback.
use super::*;

pub(super) fn remove_tree_cancellable(
    tree: &Records,
    quarantine: &File,
    refused: Refusal,
    cancel: &Cancellation,
    on_node: &mut impl FnMut(),
) -> Result<(), Refusal> {
    // matches() counts children: identity alone is blind to a same-tick arrival that keeps size and mtime.
    if !matches(tree, quarantine, OsStr::new("payload"), cancel)? {
        return Err(refused);
    }
    let mut offset = tree.end();
    let mut deleted_any = false;
    while let Some(bytes) = tree
        .previous(&mut offset)
        .changed("Trash review unreadable")?
    {
        cancel.check().map_err(|text| {
            let refusal = Refusal::cancelled(text);
            if deleted_any {
                refusal.wrap(Outcome::Partial(Op::Delete))
            } else {
                refusal
            }
        })?;
        let node = Node::decode(&bytes)?;
        let (parent, child_name) = parent_at(quarantine, OsStr::new("payload"), &node.relative)?;
        node.claim_remove(&parent, &child_name, quarantine, offset - tree.start())?;
        deleted_any = true;
        on_node();
    }
    Ok(())
}

impl Reviewed {
    pub fn reviewed_nodes(&self) -> usize {
        self.nodes
    }
    pub fn reviewed_bytes(&self) -> u64 {
        self.bytes
    }

    pub fn delete_cancellable(
        &self,
        recovery_root: &Path,
        cancel: &Cancellation,
        mut on_node: impl FnMut(),
    ) -> Result<(), Refusal> {
        cancel.check().map_err(Refusal::cancelled)?;
        let tree = self
            .tree
            .as_ref()
            .changed("Trash contents have not been reviewed.")
            .map_err(Refusal::logged)?;
        self.with_claimed(
            Op::Delete,
            recovery_root,
            None,
            || {},
            |quarantine| {
                remove_tree_cancellable(
                    tree,
                    quarantine,
                    Refusal::new(
                        Outcome::NothingDeleted,
                        "Trash contents changed; no deletion attempted.",
                    ),
                    cancel,
                    &mut on_node,
                )
            },
        )
    }
}
