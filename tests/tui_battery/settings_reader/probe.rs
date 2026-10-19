#![allow(dead_code)]

#[path = "../../../src/json.rs"]
mod json;
#[path = "../../../src/jsonstring.rs"]
mod jsonstring;
#[path = "../../../src/jsondoc.rs"]
mod jsondoc;
#[path = "../../../src/uischema.rs"]
mod uischema;
#[path = "../../../src/uimigrate.rs"]
mod uimigrate;
#[path = "../../../src/uistate.rs"]
mod uistate;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(!args.is_empty() && args.len() % 2 == 0, "provide preset/document pairs");
    for pair in args.chunks_exact(2) {
        let doc = uistate::from_file(&pair[1]);
        assert_eq!(doc.get("keys").and_then(jsondoc::Json::as_str), Some(pair[0].as_str()), "driver preset input");
        println!("settings reader accepted {}", pair[0]);
    }
}
