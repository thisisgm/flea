use super::*;
use std::process::Command;

#[test]
fn large_bambu_parts_preview_without_aborting() {
    const CHILD: &str = "FLEA_FX467_MEMORY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = Command::new("prlimit")
            .arg(format!(
                "--as={}",
                crate::backend::sandbox::ADDRESS_SPACE_BYTES
            ))
            .args(["--cpu=30", "--core=0"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "backend::model3d::memory_tests::large_bambu_parts_preview_without_aborting",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "worker aborted under its production cap: {:?}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("FX467 next preview ready"));
        return;
    }

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
    assert_eq!(parse("3mf", &single).unwrap().triangles.len(), 225_000);
    drop(single);
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
    let root = format!("<model><resources>{objects}</resources><build>{items}</build></model>");
    let mut entries = vec![("3D/3dmodel.model", root.as_bytes())];
    entries.extend(paths.iter().map(|p| (p.as_str(), part.as_bytes())));
    let bytes = testdata::archive(&entries);
    assert!(bytes.len() < MAX_INPUT);
    let model = parse_classified("3mf", &bytes)
        .expect("per-part extraction keeps the project inside the cap");
    assert_eq!(model.triangles.len(), 15 * 225_000);
    drop(model);
    drop(bytes);
    let next = parse("3mf", &testdata::project()).unwrap();
    let mut ready = Vec::new();
    wire::write(&next, &mut ready).unwrap();
    assert_eq!(wire::read(ready.as_slice()).unwrap(), next);
    println!("FX467 next preview ready");
}
