use flea::keys::user::{self, Verdict};
use flea::keys::PRESETS;
use flea::ui_logic_plugin_settings::{contributions, manifest};

const GUIDE: &str = include_str!("../PLUGINS.md");
const CHECKSUMS: &str = include_str!("../examples/plugins/checksums/manifest.json");

fn binding_example(guide: &str) -> String {
    let start = guide
        .find("```toml keys=keys.toml\n")
        .expect("the guide has a keys.toml example");
    let body = &guide[start..];
    let body = &body[body.find('\n').unwrap() + 1..];
    body[..body.find("```").expect("the example fence closes")].to_owned()
}

#[test]
fn the_guide_binding_names_the_example_plugin_action_and_is_live_in_every_preset() {
    let keys = user::parse(&binding_example(GUIDE));
    assert_eq!(keys.problems, Vec::new());
    assert_eq!(keys.bindings.len(), 1);
    let checksums = manifest::parse(CHECKSUMS, false).unwrap();
    let declared: Vec<String> = contributions::menu_actions(&checksums)
        .unwrap()
        .iter()
        .map(flea::ui_logic_custom_actions::plugins::leaf)
        .collect();
    assert!(declared.contains(&keys.bindings[0].action), "{declared:?}");
    let projection = user::Projection {
        modifiers: vec![flea::keys::CONTROL | flea::keys::SHIFT],
        samples: vec![user::Sample::new([
            Some(flea::keys::Input {
                key: "k",
                text: "",
                modifiers: flea::keys::CONTROL | flea::keys::SHIFT,
            }),
            None,
        ])
        .unwrap()],
    };
    for preset in PRESETS {
        assert_eq!(
            keys.verdicts(preset, &|id| declared.iter().any(|d| d == id), &projection),
            vec![Verdict::Live],
            "{preset}"
        );
        assert_eq!(
            keys.verdicts(preset, &|_| false, &projection),
            vec![Verdict::Inactive],
            "{preset} without the plugin"
        );
    }
}
