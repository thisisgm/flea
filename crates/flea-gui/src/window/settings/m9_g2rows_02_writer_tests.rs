//! The mounted writer is driven through its real child boundary and the core locked store.
use crate::{
    state::Shell,
    window::settings::store::{Update, Worker},
};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

const CHILD: &str = "window::settings::apply::m9_g2rows_02_writer::m9_g2rows_02_store_child";

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

impl Shell {
    pub(crate) fn m9_g2rows_02_writer(&mut self, root: &Path) -> async_channel::Receiver<Update> {
        assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
        let program = root.join("writer.sh");
        let executable = std::env::current_exe().unwrap();
        let script = format!("#!/bin/sh\n[ \"$1\" = --ui-state ] || exit 2\n/usr/bin/env -i PATH=/usr/bin:/bin M9_G2ROWS_02_ROOT={} M9_G2ROWS_02_PATCH=\"$2\" /usr/bin/setsid -w {} --exact {} --nocapture > {} 2>&1 || exit $?\ncat {}\n", quote(root.to_str().unwrap()), quote(executable.to_str().unwrap()), quote(CHILD), quote(root.join("writer-child.log").to_str().unwrap()), quote(root.join("writer-reply.json").to_str().unwrap()));
        std::fs::write(&program, script).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let worker = Worker::at(root.join("state/flea/ui.json"), program).unwrap();
        let updates = worker.updates.clone();
        self.settings.worker = Some(worker);
        updates
    }
}

#[test]
fn m9_g2rows_02_store_child() {
    let Some(root) = std::env::var_os("M9_G2ROWS_02_ROOT").map(PathBuf::from) else {
        return;
    };
    let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .join(".superpowers/m9-g2rows-02");
    assert!(
        root.is_absolute()
            && root.starts_with(&parent)
            && root != parent
            && root.join(".flea-test-sandbox").is_file()
    );
    let patch = flea::jsondoc::parse(&std::env::var("M9_G2ROWS_02_PATCH").unwrap()).unwrap();
    let store = flea::uistore::Store::at(&root.join("state"), &root.join("config"));
    let document = store.update(&patch).unwrap();
    std::fs::write(
        root.join("writer-reply.json"),
        flea::jsondoc::render(&document),
    )
    .unwrap();
}
