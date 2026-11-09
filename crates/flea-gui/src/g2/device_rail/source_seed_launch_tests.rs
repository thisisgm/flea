//! A separate test process gives startup its own lsblk fixture without mutating another test's environment.
use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const LARGE_OUTPUT: usize = 256 * 1024;
const HELPER: &str = "g2::device_rail::source::seed::launch_tests::rail_seed_launch_helper";

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && self.0.parent() == Some(Path::new("/tmp")));
        assert!(self.0.join(".flea-test-sandbox").is_file());
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn rail_seed_launch_drains_large_lsblk() {
    let made = Command::new("mktemp")
        .args(["-d", "/tmp/flea-rail-seed.XXXXXXXX"])
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = Fixture(PathBuf::from(
        String::from_utf8(made.stdout).unwrap().trim(),
    ));
    std::fs::write(root.0.join(".flea-test-sandbox"), b"startup pipe control").unwrap();
    let executable = root.0.join("lsblk");
    std::fs::write(&executable, format!(
        "#!/usr/bin/python3\nimport sys\nsys.stdout.write('{{\"blockdevices\":[],\"padding\":\"' + 'x' * {LARGE_OUTPUT} + '\"}}')\n"
    )).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", HELPER, "--nocapture"])
        .env("FLEA_RAIL_SEED_CHECK", "large")
        .env(
            "PATH",
            format!(
                "{}:{}",
                root.0.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("RAIL_SEED_LARGE_COMPLETE"));
}

#[test]
fn rail_seed_launch_helper() {
    if std::env::var("FLEA_RAIL_SEED_CHECK").as_deref() != Ok("large") {
        return;
    }
    let probe = start();
    // The control tests backpressure, so scheduling gets grace while the production wait bound remains unchanged.
    finish(probe, Instant::now() + Duration::from_secs(5));
    let seed = take().expect("healthy large lsblk output must seed DEVICES");
    let value = flea::jsondoc::parse(&seed.listing).unwrap();
    assert_eq!(
        value
            .get("padding")
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap()
            .len(),
        LARGE_OUTPUT
    );
    println!("RAIL_SEED_LARGE_COMPLETE");
}
