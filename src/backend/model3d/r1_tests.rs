use super::*;

fn material_project(resources: &str) -> Result<Model> {
    let xml = format!("<model><resources>{resources}<object id='100'><mesh><vertices><vertex x='0' y='0' z='0'/><vertex x='1' y='0' z='0'/><vertex x='0' y='1' z='0'/></vertices><triangles><triangle v1='0' v2='1' v3='2'/></triangles></mesh></object></resources><build><item objectid='100'/></build></model>");
    parse("3mf",&testdata::archive(&[("3D/3dmodel.model",xml.as_bytes())]))
}

#[test]
fn material_combination_names_count_utf8_bytes_and_separators_before_join() {
    for composite in [false,true] {
        for (first,second) in [("é".repeat(255),"b".repeat(511)),(String::new(),"b".repeat(1021))] {
            for extra in ["","b"] {
                let second = format!("{second}{extra}");
                let resources = if composite {
                    format!("<basematerials id='1'><base name='{first}'/><base name='{second}'/></basematerials><compositematerials id='3' matid='1'><composite values='0.5 0.5'/></compositematerials>")
                } else {
                    format!("<basematerials id='1'><base name='{first}'/></basematerials><colorgroup id='2'><color name='{second}' color='#123456'/></colorgroup><multiproperties id='3' pids='1 2'><multi pindices='0 0'/></multiproperties>")
                };
                let result = material_project(&resources);
                if extra.is_empty() {
                    let model = result.unwrap();
                    assert_eq!(model.materials.last().unwrap().name,format!("{first} + {second}"));
                    assert_eq!(model.materials.last().unwrap().name.len(),1024);
                } else {
                    assert_eq!(result.unwrap_err(),"3D material name exceeds its bound");
                }
            }
        }
    }
}

#[test]
fn material_names_bound_derived_inputs_and_keep_empty_components() {
    for width in [509,510] {
        let first = "a".repeat(width);
        let second = "b".repeat(509);
        let resources = format!("<basematerials id='1'><base name='{first}'/><base name='{second}'/></basematerials><colorgroup id='2'><color name='' color='#123456'/></colorgroup><multiproperties id='4' pids='3 2'><multi pindices='0 0'/></multiproperties><compositematerials id='3' matid='1'><composite values='0.5 0.5'/></compositematerials>");
        let result = material_project(&resources);
        if width == 509 {
            assert_eq!(result.unwrap().materials.last().unwrap().name,format!("{first} + {second} + "));
        } else {
            assert_eq!(result.unwrap_err(),"3D material name exceeds its bound");
        }
    }
    let model = material_project("<basematerials id='1'><base name=''/><base name=''/></basematerials><compositematerials id='3' matid='1'><composite values='0.5 0.5'/></compositematerials>").unwrap();
    assert_eq!(model.materials.last().unwrap().name," + ");
}

#[test]
fn material_resource_types_are_validated_before_any_combination_expands() {
    for reference in [
        "<compositematerials id='5' matid='2'/>",
        "<compositematerials id='5' matid='3'/>",
        "<compositematerials id='5' matid='4'/>",
        "<compositematerials id='5' matid='8'/>",
        "<compositematerials id='5' matid='9'/>",
        "<compositematerials id='5' matid='99'/>",
        "<multiproperties id='5' pids='4'/>",
        "<multiproperties id='5' pids='5'/>",
        "<multiproperties id='5' pids='8'/>",
        "<multiproperties id='5' pids='9'/>",
        "<multiproperties id='5' pids='99'/>",
    ] {
        let xml = format!("<resources><basematerials id='1'/><colorgroup id='2'/><object id='8'/><texture2dgroup id='9'/><compositematerials id='3' matid='1'><composite values='0.5 0.5'/></compositematerials><multiproperties id='4' pids='1'><multi pindices='0'/></multiproperties>{reference}</resources>");
        let doc = xml::parse(&xml).unwrap();
        let mut model = Model {
            materials: vec![Material { name: "a".repeat(600), color: None };2],
            ..Model::default()
        };
        let mut groups = BTreeMap::from([(1,vec![0,1]),(2,vec![0])]);
        assert_eq!(properties::resolve(&doc,doc.root,&mut groups,&mut model).unwrap_err(),
            "unsupported or missing 3MF material resource reference","{reference}");
        assert_eq!(model.materials.len(),2,"{reference}");
        assert_eq!(groups.len(),2,"{reference}");
    }
}

fn numbered(source: &str, enabled: bool) -> String {
    if !enabled { return source.into() }
    source.lines().enumerate().map(|(i,line)| format!("N{}{line}\n",i+1)).collect()
}

fn toolpath(format: &str, source: &str) -> Result<Model> {
    if format == "bgcode" { parse(format,&testdata::bgcode(0,0,source.as_bytes(),source.len())) }
    else { parse(format,source.as_bytes()) }
}

#[test]
fn tool_and_feed_only_commands_preserve_arcs_without_dispatching_them() {
    for arc in [2,3] {
        for numbers in [false,true] {
            for change in ["T1","F600","T1 F600"] {
                let prefix = format!("G90\nM83\nG1 X10 F1200\nG{arc} X0 Y10 I-10 J0 E1\n");
                let tail = format!("G{arc} I10 J0 E1\nE1 I10 J0\n");
                let source = numbered(&format!("{prefix}{change}\nX-10 Y0 I0 J-10 E1\n{tail}"),numbers);
                let control = numbered(&format!("{prefix}X-10 Y0 I0 J-10 E1 {change}\n{tail}"),numbers);
                for format in ["gcode","bgcode"] {
                    let expected = toolpath(format,&control).unwrap();
                    let result = toolpath(format,&source).unwrap();
                    assert_eq!(result.segments,expected.segments,"{format}, G{arc}, numbered={numbers}, {change}");
                    assert_eq!(result.layers,expected.layers);
                    let last = result.segments.last().unwrap();
                    assert_eq!(last.end,[-10.,0.,0.]);
                    assert_eq!(last.tool,if change.contains('T') { 1 } else { 0 });
                    assert_eq!(last.feed_mm_min,if change.contains('F') { 600. } else { 1200. });
                    assert!(result.segments.len() > toolpath(format,&prefix).unwrap().segments.len()+100);
                }
            }
        }
    }
}

#[test]
fn non_motion_guard_preserves_tool_feed_and_explicit_arc_validation() {
    for command in ["T256","T-1","T1.5","F0","F-1","G3"] {
        for numbers in [false,true] {
            let source = numbered(&format!("G90\nM83\nG1 X10 F1200\nG3 X0 Y10 I-10 J0 E1\n{command}\n"),numbers);
            for format in ["gcode","bgcode"] {
                assert!(toolpath(format,&source).is_err(),"{format}, numbered={numbers}, {command}");
            }
        }
    }
}
