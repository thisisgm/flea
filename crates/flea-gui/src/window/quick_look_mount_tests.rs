//! Every model extension, in any case, mounts the model viewer, and nothing else does.
use super::Mount;
use flea::protocol::RowFacts;

fn mount(name: &str) -> Option<Mount> {
    Mount::of(&RowFacts {
        name: name.into(),
        ..RowFacts::default()
    })
}

#[test]
fn every_model_extension_mounts_the_model_viewer() {
    for name in [
        "a.stl",
        "a.3mf",
        "a.obj",
        "a.ply",
        "a.amf",
        "a.gcode",
        "a.gco",
        "a.bgcode",
        "MODEL.STL",
        "B.3MF",
        "part.v2.stl",
    ] {
        assert!(matches!(mount(name), Some(Mount::Model)), "{name}");
    }
    for name in ["notes.md", "model.txt", "stl", "a.stlx", "a.stl.bak"] {
        assert!(!matches!(mount(name), Some(Mount::Model)), "{name}");
    }
}
