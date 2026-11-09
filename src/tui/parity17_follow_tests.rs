use crate::{
    jsondoc,
    tui::{
        action_key_tests::{drain, listing},
        actions,
        wire::{self, count, text},
    },
};
fn parse(s: &str) -> crate::jsondoc::Json {
    jsondoc::parse(s).unwrap()
}

// A link to / opens / itself; a broken link says its target is gone and stays where it is.
#[test]
fn follow_opens_root_and_names_a_broken_target() {
    for (directory, name, opened) in [
        ("/", "", Some("/")),
        ("/nonexistent-flea-follow", "gone", None),
    ] {
        let (mut w, reader) = wire::echo_wire();
        let mut m = listing(&mut w, "yazi");
        m.rows.get_mut(&0).unwrap().link = "target".into();
        actions::act(&mut m, "followLink", &mut w).unwrap();
        let id = count(
            drain(&mut w)
                .iter()
                .find(|v| text(v, "c") == "linktarget")
                .unwrap(),
            "id",
        );
        m.receive(parse(&format!(r#"{{"t":"linktarget","id":{id},"path":"/listing/folder","directory":"{directory}","name":"{name}"}}"#)), &mut w).unwrap();
        assert_eq!(
            m.pending.as_deref(),
            opened.map(std::path::Path::new),
            "{directory}{name}"
        );
        assert!(m.restore_path.is_none());
        if opened.is_none() {
            assert_eq!(
                m.error,
                "That link points to an item that no longer exists."
            );
        }
        wire::finish(w, reader);
    }
}
