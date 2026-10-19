// Cargo builds the production backend in the same job as these isolated protocol fixtures.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[allow(dead_code)]
#[path = "../src/oflags.rs"]
mod oflags;
#[allow(dead_code)]
#[path = "../src/json.rs"]
mod json;
#[path = "../src/jsonstring.rs"]
mod jsonstring;
#[allow(dead_code)]
#[path = "../src/jsondoc.rs"]
mod jsondoc;
mod backend {
    #[allow(dead_code)]
    pub mod testdir { include!("../src/backend/testdir.rs"); }
}
#[path = "../src/backend/searchprior_protocol_tests.rs"]
mod protocol_tests;
