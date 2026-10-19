use super::*;

impl Reviewed {
    pub fn delete(&self, recovery_root: &Path) -> Result<(), Refusal> {
        self.delete_after(recovery_root, || {})
    }
    pub(super) fn delete_after(&self, recovery_root: &Path, before_claim: impl FnOnce()) -> Result<(), Refusal> {
        self.delete_with(recovery_root, before_claim, |_| {})
    }
    pub(super) fn delete_with(&self, recovery_root: &Path, before_claim: impl FnOnce(), before_remove: impl FnOnce(&Path)) -> Result<(), Refusal> {
        let tree = self.tree.as_ref().changed("Trash contents have not been reviewed.").map_err(Refusal::logged)?;
        self.with_claimed(Op::Delete, recovery_root, None, before_claim, |quarantine| {
            before_remove(&fd_path(quarantine, OsStr::new("payload")));
            remove_tree(tree, quarantine, Refusal::new(Outcome::NothingDeleted, "Trash contents changed; no deletion attempted."))
        })
    }
}
