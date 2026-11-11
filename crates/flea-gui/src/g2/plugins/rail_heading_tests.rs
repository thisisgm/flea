//! Board 0402-plugin-rail-mark on the real rail: a place row under its own title or PLUGINS, painting its own mark.
use super::rail_place_fixture::*;
use crate::state::Shell;
use flea::{
    jsondoc::Json,
    ui_logic_rail_places::{order::Heading, plugin::PLUGINS_HEADING},
};
use gpui::{Entity, TestAppContext, VisualTestContext};

// Each drawn heading with its row labels, as the rail sets them.
fn drawn(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<String> {
    shell.read_with(cx, |s, cx| {
        let producer = s.rail.as_ref().unwrap().places.read(cx);
        producer
            .provider
            .drawn
            .iter()
            .map(|s| {
                let labels: Vec<_> = s.rows.iter().map(|r| r.label.to_string()).collect();
                format!("{}:{}", s.heading, labels.join(","))
            })
            .collect()
    })
}

fn retitle(fixture: &mut Fixture, title: &str) {
    for (key, block) in &mut fixture.plugin.manifest.blocks {
        if let (true, Json::Obj(fields)) = (key == "place", block) {
            fields.retain(|(name, _)| name != "title");
            fields.push(("title".into(), Json::Str(title.into())));
        }
    }
    fixture.publish();
}

#[gpui::test]
fn an_untitled_place_row_stands_under_plugins_and_paints_its_own_mark(cx: &mut TestAppContext) {
    let (mut fixture, shell, cx) = Fixture::open("virtual-place", cx);
    fixture.pump(cx);
    frame(cx);
    assert_eq!(
        drawn(&shell, cx).last().map(String::as_str),
        Some("PLUGINS:Example Files"),
        "0.4.1 stood the place row alone with no heading"
    );
    shell.read_with(cx, |s, cx| {
        let producer = s.rail.as_ref().unwrap().places.read(cx);
        let section = producer.sections.last().unwrap();
        assert_eq!(section.heading, Heading::Key(PLUGINS_HEADING));
        let mark = section.rows[0]
            .mark
            .as_ref()
            .expect("the example ships mark.svg");
        assert!(mark
            .path
            .ends_with("examples/plugins/virtual-place/mark.svg"));
        let index = producer
            .entries()
            .iter()
            .position(|r| r.label == "Example Files")
            .unwrap();
        let laid = producer.laid.borrow();
        let row = laid.row(index).expect("the rail lays out the place row");
        let (key, data) = row
            .owned
            .as_ref()
            .expect("the laid row paints the owned mark");
        assert!(key.starts_with("flea-rail-mark:"));
        assert_eq!(&data[..], &mark.svg[..]);
        assert_eq!(row.glyph, "folder", "the declared glyph stays the fallback");
    });
}

#[gpui::test]
fn a_place_title_names_its_heading_unless_it_reads_as_a_built_in_one(cx: &mut TestAppContext) {
    let (mut fixture, shell, cx) = Fixture::open("virtual-place", cx);
    fixture.pump(cx);
    retitle(&mut fixture, "Cloud");
    fixture.pump(cx);
    frame(cx);
    assert_eq!(
        drawn(&shell, cx).last().map(String::as_str),
        Some("CLOUD:Example Files")
    );
    for clash in ["devices", "Plugins", "Favorites"] {
        retitle(&mut fixture, clash);
        fixture.pump(cx);
        frame(cx);
        let drawn = drawn(&shell, cx);
        assert_eq!(
            drawn.last().map(String::as_str),
            Some("PLUGINS:Example Files"),
            "{clash} never names a second built-in heading"
        );
        assert_eq!(
            drawn.iter().filter(|s| s.contains("Example Files")).count(),
            1
        );
    }
}
