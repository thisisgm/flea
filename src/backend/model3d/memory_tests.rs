use super::*;
use crate::backend::testdir::TestDir;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn large_bambu_parts_preview_without_aborting() {
    const CHILD: &str = "FLEA_FX467_MEMORY_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        preview(child_root(&root));
        return;
    }

    let (_sandbox, root) = fixture_root("model3d-memory");
    fixtures(&root);

    // Debug XML parsing approaches the production CPU limit; this test isolates address-space use.
    let output = Command::new("prlimit")
        .arg(format!(
            "--as={}",
            crate::backend::sandbox::ADDRESS_SPACE_BYTES
        ))
        .arg("--core=0")
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "backend::model3d::memory_tests::large_bambu_parts_preview_without_aborting",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, &root)
        .output()
        .unwrap();
    assert_child_finished(
        &output,
        "worker aborted under its production memory cap",
        "FX467 next preview ready",
    );
}

// Both failure paths retain the child status and captured diagnostics.
pub(super) fn assert_child_finished(output: &std::process::Output, context: &str, marker: &str) {
    let details = || {
        format!(
            "{context}: {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    };
    assert!(output.status.success(), "{}", details());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(marker),
        "child omitted {marker}; {}",
        details()
    );
}

pub(super) fn fixture_root(tag: &str) -> (TestDir, PathBuf) {
    let sandbox = TestDir::new(tag);
    let made = Command::new("mktemp")
        .arg("-d")
        .arg(sandbox.join("archives.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(made.status.success(), "mktemp creates the fixture root");
    let root = PathBuf::from(std::str::from_utf8(&made.stdout).unwrap().trim());
    sandbox.assert_contains(&root);
    write_fixture(&root, ".flea-test-sandbox", b"3MF memory fixtures\n");
    (sandbox, root)
}

// The capped child reads only the marked root its parent made, and names the root it refuses.
pub(super) fn child_root(root: &std::ffi::OsStr) -> &Path {
    let root = Path::new(root);
    assert!(
        root.is_absolute() && root.join(".flea-test-sandbox").is_file(),
        "child fixture root is not an absolute marked directory: {}",
        root.display()
    );
    root
}

pub(super) fn read_fixture(root: &Path, name: &str) -> Vec<u8> {
    let path = root.join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("read fixture {}: {error}", path.display()))
}

pub(super) fn write_fixture(root: &Path, name: &str, bytes: &[u8]) {
    let path = root.join(name);
    std::fs::write(&path, bytes)
        .unwrap_or_else(|error| panic!("write fixture {}: {error}", path.display()));
}

fn fixtures(root: &Path) {
    let main = b"<model><resources><object id='10'><components><component objectid='1' path='/3D/Objects/part00.model'/></components></object></resources><build><item objectid='10'/></build></model>";
    let mut part = String::from("<model><resources><object id='1'><mesh><vertices><vertex x='0' y='0' z='0'/><vertex x='1' y='0' z='0'/><vertex x='0' y='1' z='0'/></vertices><triangles>");
    for _ in 0..225_000 {
        part.push_str("<triangle v1='0' v2='1' v3='2'/>");
    }
    part.push_str("</triangles></mesh></object></resources><build/></model>");
    let single = testdata::archive(&[
        ("3D/3dmodel.model", &main[..]),
        ("3D/Objects/part00.model", part.as_bytes()),
    ]);
    write_fixture(root, "single.3mf", &single);
    let paths: Vec<_> = (0..15)
        .map(|i| format!("3D/Objects/part{i:02}.model"))
        .collect();
    let objects: String = paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            format!(
                "<object id='{}'><components><component objectid='1' path='/{path}'/></components></object>",
                i + 2
            )
        })
        .collect();
    let items: String = (2..17).map(|i| format!("<item objectid='{i}'/>")).collect();
    let main = format!("<model><resources>{objects}</resources><build>{items}</build></model>");
    let mut entries = vec![("3D/3dmodel.model", main.as_bytes())];
    entries.extend(paths.iter().map(|p| (p.as_str(), part.as_bytes())));
    let bytes = testdata::archive(&entries);
    assert!(bytes.len() < MAX_INPUT);
    write_fixture(root, "multi.3mf", &bytes);
    write_fixture(root, "next.3mf", &testdata::project());
}

fn preview(root: &Path) {
    let single = read_fixture(root, "single.3mf");
    assert_eq!(parse("3mf", &single).unwrap().triangles.len(), 225_000);
    drop(single);
    let bytes = read_fixture(root, "multi.3mf");
    let model = parse_classified("3mf", &bytes)
        .expect("per-part extraction keeps the project inside the cap");
    assert_eq!(model.triangles.len(), 15 * 225_000);
    drop(model);
    drop(bytes);
    let bytes = read_fixture(root, "next.3mf");
    let next = parse("3mf", &bytes).unwrap();
    let mut ready = Vec::new();
    wire::write(&next, &mut ready).unwrap();
    assert_eq!(wire::read(ready.as_slice()).unwrap(), next);
    println!("FX467 next preview ready");
}
