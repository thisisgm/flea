use super::*;

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
    let name = "N".repeat(600);
    let source = format!("<model><resources><basematerials id='1'><base name='{name}'/></basematerials><basematerials id='2'><base name='{name}'/></basematerials><multiproperties id='3' pids='1 2'><multi pindices='0 0'/></multiproperties></resources><build/></model>");
    let bytes = super::testdata::archive(&[("3D/3dmodel.model",source.as_bytes())]);
    assert_eq!(parse("3mf",&bytes).unwrap_err(), "3D material name exceeds its bound");
}
