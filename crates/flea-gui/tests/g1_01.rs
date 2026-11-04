use flea::jsondoc::{self, Json};
use flea_gui::{
    backend::{Backend, Event},
    window::{
        composition::Geometry,
        lifecycle::{Lifecycle, Phase},
        locale::Service,
        startup::{Arguments, Launch, Settings, View, INSPECT_WAIT_MS, LISTING_WAIT_MS},
    },
};
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

fn settings(text: &str) -> Settings {
    Settings::from_json(jsondoc::parse(text).unwrap())
}

#[path = "g1_01/lifecycle.rs"]
mod lifecycle;
#[path = "g1_01/locale.rs"]
mod locale;
#[path = "g1_01/shutdown.rs"]
mod shutdown;
#[path = "g1_01/startup.rs"]
mod startup;

fn root() -> PathBuf {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g1-01");
    std::fs::create_dir_all(&parent).unwrap();
    let output = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("test.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), b"g1-01\n").unwrap();
    root
}
