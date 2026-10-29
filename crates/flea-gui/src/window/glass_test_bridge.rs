//! Test-only access to the production presentation input reader and resolver.
use super::{Snapshot, Sources};
use std::path::PathBuf;

impl Snapshot {
    pub(crate) fn glass_fixture(paths: &[PathBuf], cache: PathBuf) -> Self {
        let sources = Sources::at(std::array::from_fn(|at| paths[at].clone()), cache);
        let inputs = sources
            .read()
            .expect("the marked glass inputs are readable");
        sources.resolve(&inputs, "monospace").0
    }
}
