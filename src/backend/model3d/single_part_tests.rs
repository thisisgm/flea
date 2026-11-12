use super::*;
use std::process::Command;

#[test]
fn xml_fits_beside_the_existing_archive_bounds() {
    const CHILD: &str = "FLEA_FX467_ATTRIBUTES_CHILD";
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
                "backend::model3d::single_part_tests::xml_fits_beside_the_existing_archive_bounds",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "padded document refused: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("FX467 padded attributes ready"));
        return;
    }
    let attrs: String = ('A'..='K').map(|key| format!(" {key}='1'")).collect();
    let node = format!("<x{attrs}/>");
    let xml = format!("<model>{}</model>", node.repeat(958_000)).into_boxed_str();
    assert!(xml.len() < 64 * 1024 * 1024);
    // Compressed input and expanded members can coexist at their existing container bounds.
    let retained_bytes = MAX_INPUT + MAX_EXPANDED;
    let retained = vec![0u8; retained_bytes - xml.len()];
    let mut budget = xml::Budget::new();
    budget.retained(retained_bytes);
    budget.begin_document();
    let doc =
        xml::parse_bounded(&xml, &mut budget).expect("legacy-sized XML fits beside its archive");
    assert_eq!(doc.children(doc.root, "x").count(), 958_000);
    assert_eq!(doc.nodes[1].attr("K"), Some("1"));
    std::hint::black_box(retained);
    println!("FX467 padded attributes ready");
}

#[test]
fn a_single_part_at_the_document_bound_still_previews() {
    const CHILD: &str = "FLEA_FX467_SINGLE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = Command::new("prlimit")
            .arg(format!("--as={}", crate::backend::sandbox::ADDRESS_SPACE_BYTES))
            .args(["--cpu=30", "--core=0"])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "backend::model3d::single_part_tests::a_single_part_at_the_document_bound_still_previews", "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output().unwrap();
        assert!(
            output.status.success(),
            "single part refused: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("FX467 single part ready"));
        return;
    }
    let mut part = String::from("<model><resources><object id='1'><mesh><vertices>");
    for i in 0..300_000 {
        part.push_str(&format!(
            "<vertex x='{}' y='{}' z='{}'/>",
            i % 200,
            i % 199,
            i % 100
        ));
    }
    part.push_str("</vertices><triangles>");
    for i in 0..600_000 {
        let a = i % 300_000;
        part.push_str(&format!(
            "<triangle v1='{a}' v2='{}' v3='{}'/>",
            (a + 1) % 300_000,
            (a + 7) % 300_000
        ));
    }
    part.push_str(
        "</triangles></mesh></object></resources><build><item objectid='1'/></build></model>",
    );
    let bytes = testdata::archive(&[("3D/3dmodel.model", part.as_bytes())]);
    let model = parse_classified("3mf", &bytes).expect("legacy-compatible single part");
    assert_eq!(model.triangles.len(), 600_000);
    wire::write(&model, std::io::sink()).unwrap();
    println!("FX467 single part ready");
}
