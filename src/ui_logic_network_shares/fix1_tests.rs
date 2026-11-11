use super::{browser::Browser, form::Form};

#[test]
fn fix1_loaded_and_label_only_round_trips_preserve_encoded_resources() {
    for uri in [
        "sftp://user@host/a%2Fb",
        "sftp://user@host/%2E%2E/file",
        "smb://WORK;user@host/a%2520b",
        "davs://host/a%23b",
        "ftp://user@host/a%2Fb",
    ] {
        let address = Form::saved_address(uri).unwrap();
        let mut form = Form::load(&address, "Saved");
        assert_eq!(form.uri().unwrap(), address.uri);
        form.label = "Renamed".into();
        assert_eq!(form.uri().unwrap(), address.uri);
        form.path.push_str(" changed");
        assert_ne!(form.uri().unwrap(), address.uri);
        assert!(form.uri().unwrap().ends_with("%20changed"));
    }
}

#[test]
fn fix1_share_names_are_literal_single_components() {
    for (name, encoded) in [
        ("a%20b", "a%2520b"),
        ("a b", "a%20b"),
        ("a#b", "a%23b"),
        ("a?b", "a%3Fb"),
        ("%", "%25"),
        (" media ", "%20media%20"),
    ] {
        let mut browser = Browser::default();
        browser.show("smb://host/".into(), "Host".into(), vec![name.into()], 7);
        let (uri, label, origin) = browser.activate().unwrap();
        assert_eq!(uri, format!("smb://host/{encoded}/"));
        assert_eq!(label, name);
        assert_eq!(origin, 7);
        assert_eq!(
            super::browser::child_name("smb://host/", &uri).as_deref(),
            Some(name)
        );
    }
    for name in ["", ".", "..", "a/b", "a\\b", "a\nb", "a\0b"] {
        let mut browser = Browser::default();
        browser.show("smb://host/".into(), "Host".into(), vec![name.into()], 7);
        assert!(browser.activate().is_none(), "{name:?}");
    }
}
