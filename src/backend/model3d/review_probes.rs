use super::*;
use std::process::Command;

#[test]
fn r1_tool_change_after_arc_keeps_valid_toolpath() {
    let source = "G90\nM83\nG1 X10 F1200\nG3 X0 Y10 I-10 J0 E1\nT1\nG1 X1 E1\n";
    let control = source.replace("T1\n", "G1\nT1\n");
    assert!(parse("gcode", control.as_bytes()).is_ok());
    let result = parse("gcode", source.as_bytes());
    assert!(result.is_ok(), "valid tool change after arc was rejected: {result:?}");
}

#[test]
fn r1_material_names_refuse_before_amplifying() {
    const CHILD: &str = "FLEA_R1_MATERIAL_PROBE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let mut xml = String::from("<model><resources><basematerials id='1'><base name='N'/></basematerials>");
        for id in 2..=6 {
            let ids = std::iter::repeat((id-1).to_string()).take(64).collect::<Vec<_>>().join(" ");
            let indices = std::iter::repeat("0").take(64).collect::<Vec<_>>().join(" ");
            xml.push_str(&format!("<multiproperties id='{id}' pids='{ids}'><multi pindices='{indices}'/></multiproperties>"));
        }
        xml.push_str("<object id='100'><mesh><vertices><vertex x='0' y='0' z='0'/><vertex x='1' y='0' z='0'/><vertex x='0' y='1' z='0'/></vertices><triangles><triangle v1='0' v2='1' v3='2'/></triangles></mesh></object></resources><build><item objectid='100'/></build></model>");
        eprintln!("material probe XML: {} bytes, five groups", xml.len());
        let bytes = super::testdata::archive(&[("3D/3dmodel.model",xml.as_bytes())]);
        assert!(parse("3mf",&bytes).is_err(), "material name bound must reject before expansion");
        return;
    }
    let output = Command::new("prlimit").args(["--as=268435456","--cpu=3","--core=0","--"])
        .arg(std::env::current_exe().unwrap()).args(["--exact","backend::model3d::review_probes::r1_material_names_refuse_before_amplifying","--nocapture"])
        .env(CHILD,"1").output().unwrap();
    eprintln!("bounded child status: {}\n{}", output.status,String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "small material graph exceeded allocation bound instead of returning an error");
}
