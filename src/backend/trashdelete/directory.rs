use super::super::file_claim::{changed, FileClaim, InodeMarks};
use super::*;

impl PathReview {
    // Moving a FUSE directory can replace every child's kernel inode, so only empty nodes are claimed.
    pub(super) fn remove_directory(
        &self,
        parent: &File,
        name: &OsStr,
        quarantine: &File,
        held: &FileClaim,
        before_claim: impl FnOnce(),
    ) -> Result<(), Refusal> {
        before_claim();
        if !held.unchanged() || !matches(&self.tree, parent, name, &Cancellation::default())? {
            return Err(changed());
        }
        let marks = InodeMarks::new()?;
        let directories = self.directory_marks(parent, name, held, &marks)?;
        let mut offset = self.tree.end();
        let mut deleted_any = false;
        let result = (|| {
            while let Some(bytes) = self
                .tree
                .previous(&mut offset)
                .changed("Trash review unreadable")?
            {
                if !held.unchanged() {
                    return Err(changed());
                }
                let node = Node::decode(&bytes)?;
                let (node_parent, node_name) = parent_at(parent, name, &node.relative)?;
                if !node.relative.as_os_str().is_empty() {
                    let ancestor = node.relative.parent().ok_or_else(changed)?;
                    let expected = directories.get(ancestor).ok_or_else(changed)?;
                    if marks.descriptor(node_parent.as_raw_fd())? != *expected {
                        return Err(changed());
                    }
                }
                let proof = directories
                    .get(&node.relative)
                    .map(|watch| (&marks, *watch));
                node.claim_remove_held(
                    &node_parent,
                    &node_name,
                    quarantine,
                    offset - self.tree.start(),
                    proof,
                )?;
                deleted_any = true;
            }
            Ok(())
        })();
        result.map_err(|error| {
            if deleted_any {
                error.wrap(Outcome::PartialAtOriginal)
            } else {
                error
            }
        })
    }

    fn directory_marks(
        &self,
        parent: &File,
        name: &OsStr,
        held: &FileClaim,
        marks: &InodeMarks,
    ) -> Result<std::collections::BTreeMap<PathBuf, i32>, Refusal> {
        let mut result = std::collections::BTreeMap::new();
        let mut cursor = self.tree.start();
        while let Some(bytes) = self
            .tree
            .next(&mut cursor)
            .changed("Trash review unreadable")?
        {
            let node = Node::decode(&bytes)?;
            if !node.directory() {
                continue;
            }
            let (parent, name) = parent_at(parent, name, &node.relative)?;
            let path = fd_path(&parent, &name);
            let watch = if node.relative.as_os_str().is_empty() {
                held.kernel_mark(marks)?
            } else {
                marks.path(&path)?
            };
            if identity(&path).map_err(|_| changed())? != node.identity
                || marks.path(&path)? != watch
            {
                return Err(changed());
            }
            result.insert(node.relative, watch);
        }
        Ok(result)
    }
}
