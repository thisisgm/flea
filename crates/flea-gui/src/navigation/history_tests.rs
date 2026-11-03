use super::parent_of;

#[test]
fn a_plugin_place_climbs_to_its_root_and_stops_there() {
    for (path, parent) in [
        (
            "plugin://example.virtual-place/a/b",
            "plugin://example.virtual-place/a",
        ),
        (
            "plugin://example.virtual-place/a/",
            "plugin://example.virtual-place/",
        ),
        (
            "plugin://example.virtual-place/a",
            "plugin://example.virtual-place/",
        ),
        (
            "plugin://example.virtual-place/",
            "plugin://example.virtual-place/",
        ),
        (
            "plugin://example.virtual-place",
            "plugin://example.virtual-place",
        ),
        ("/home/u", "/home"),
        ("/", "/"),
    ] {
        assert_eq!(parent_of(path), parent, "{path}");
    }
    assert!(
        crate::columns::geometry::ancestors("plugin://example.virtual-place/", 4).is_empty(),
        "no local column is drawn beside a place's root"
    );
    assert_eq!(
        crate::columns::geometry::ancestors("plugin://example.virtual-place/a", 4),
        vec!["plugin://example.virtual-place/".to_owned()]
    );
}
