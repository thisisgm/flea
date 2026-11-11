use super::*;

#[gpui::test]
fn uish_misc_menu_brand_reader_and_peer_glyph_match_the_built_rows(cx: &mut TestAppContext) {
    with(cx, |menu, env, window| {
        let entries = listing_entries(&Context {
            row_is_file: true,
            has_row: true,
            taildrop_installed: true,
            taildrop_code: "ready".into(),
            taildrop_peers: vec![flea::ui_logic_context_menus::build::Peer {
                id: "peer".into(),
                label: "Alpha".into(),
            }],
            ..Default::default()
        });
        menu.open(Face::File, entries, (4., 4.), WIDE, "a", env, window);
        let data = super::super::ipc::joined::model(menu.card.as_ref(), env.catalog);
        let data = flea::jsondoc::parse(&data).unwrap();
        let row = data
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| {
                entry.get("action").and_then(flea::jsondoc::Json::as_str) == Some("taildrop")
            })
            .unwrap();
        assert_eq!(
            row.get("mark").and_then(flea::jsondoc::Json::as_str),
            Some("tailscale")
        );
        assert_eq!(row.get("glyph"), Some(&flea::jsondoc::Json::Null));
        assert_eq!(
            row.get("submenu").unwrap().as_array().unwrap()[0]
                .get("glyph")
                .and_then(flea::jsondoc::Json::as_str),
            Some("server")
        );
    });
}
