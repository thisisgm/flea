use super::*;
use std::path::Path;
use std::process::Command;

#[test]
fn an_oversized_document_requires_a_typed_refusal() {
    const CHILD: &str = "FLEA_FX467_REFUSAL_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = super::memory_tests::child_root(&root);
        let bytes = super::memory_tests::read_fixture(root, "oversized.3mf").into_boxed_slice();
        let failure = match parse_classified("3mf", &bytes) {
            Ok(model) => panic!(
                "expected typed refusal, parsed {} triangles",
                model.triangles.len()
            ),
            Err(failure) => failure,
        };
        assert_eq!(failure.refusal, Some(Refusal::TooLarge));
        assert_eq!(failure.reason, "XML storage exceeds its bound");
        println!("FX467 typed refusal");
        return;
    }

    let (_sandbox, root) = super::memory_tests::fixture_root("model3d-refusal");
    fixture(&root);
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
            "backend::model3d::refusal_tests::an_oversized_document_requires_a_typed_refusal",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, &root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "refusal worker failed: {:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("FX467 typed refusal"));
}

fn fixture(root: &Path) {
    let mut xml = String::from("<model><resources>");
    let keys: Vec<_> = ('A'..='Z').chain('a'..='g').collect();
    let attrs: String = keys.iter().map(|key| format!(" {key}='1'")).collect();
    let node = format!("<x{attrs}/>");
    for _ in 0..330_000 {
        xml.push_str(&node);
    }
    xml.push_str("<object id='1'><mesh><vertices><vertex x='0' y='0' z='0'/><vertex x='1' y='0' z='0'/><vertex x='0' y='1' z='0'/></vertices><triangles><triangle v1='0' v2='1' v3='2'/></triangles></mesh></object></resources><build><item objectid='1'/></build></model>");
    assert!(xml.len() < 64 * 1024 * 1024);
    let opaque = vec![0; 48 * 1024 * 1024];
    let bytes = testdata::archive(&[
        ("3D/3dmodel.model", xml.as_bytes()),
        ("Metadata/opaque.bin", &opaque),
    ]);
    super::memory_tests::write_fixture(root, "oversized.3mf", &bytes);
}
