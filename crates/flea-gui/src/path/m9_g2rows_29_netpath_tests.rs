//! The path bar's Enter classifies a user@host:/path line as a network address before any local resolution.
use super::entry::{Commit, Entry};
use super::{completion_dir, is_network_uri};
use crate::g2::pl2::routing::{address, AddressRoute};

const CURRENT: &str = "/fixture/current";
const HOME: &str = "/fixture/home";

fn commit(line: &str) -> Commit {
    let mut entry = Entry::default();
    assert!(entry.start(CURRENT));
    entry.replace(None, line);
    entry.commit(CURRENT, HOME, false)
}

#[test]
fn m9_g2rows_29_shorthand_commits_as_network_and_routes_to_the_shared_opener() {
    for (line, uri) in [
        ("user@east:/work", "user@east:/work"),
        ("  gm@nas.local:/srv/media  ", "gm@nas.local:/srv/media"),
        ("user@east:/", "user@east:/"),
        ("user@[::1]:/path", "user@[::1]:/path"),
        ("smb://server.lan/share", "smb://server.lan/share"),
        ("SFTP://u@host:2222/a%2fb", "SFTP://u@host:2222/a%2fb"),
        ("ftp://host/pub", "ftp://host/pub"),
        ("davs://host/dav", "davs://host/dav"),
    ] {
        assert_eq!(commit(line), Commit::Network(uri.into()), "{line}");
        assert!(is_network_uri(line), "{line}");
    }
    let Commit::Network(uri) = commit("user@east:/work") else {
        unreachable!();
    };
    let AddressRoute::Network(request) = address(&uri, 7) else {
        panic!("the committed shorthand is refused by the network opener");
    };
    assert_eq!(
        (request.uri.as_str(), request.label.as_str()),
        ("sftp://user@east/work", "")
    );
    assert_eq!(
        completion_dir("user@east:/wo", CURRENT, HOME),
        "user@east:/"
    );
}

#[test]
fn m9_g2rows_29_local_colon_relative_explicit_and_malformed_lines_stay_local() {
    for (line, path) in [
        ("notes:draft", "/fixture/current/notes:draft"),
        ("user@host:relative", "/fixture/current/user@host:relative"),
        ("user@:/x", "/fixture/current/user@:/x"),
        ("@east:/work", "/fixture/current/@east:/work"),
        ("user@bad_host!:/x", "/fixture/current/user@bad_host!:/x"),
        ("me@mail", "/fixture/current/me@mail"),
        ("./user@east:/work", "/fixture/current/user@east:/work"),
        ("../user@east:/work", "/fixture/user@east:/work"),
        ("~/user@east:/work", "/fixture/home/user@east:/work"),
        ("/srv/user@east:/work", "/srv/user@east:/work"),
        ("file:///srv/a@b:/c", "/srv/a@b:/c"),
        ("https://host/", "/fixture/current/https:/host"),
        ("~", HOME),
    ] {
        assert_eq!(commit(line), Commit::Open(path.into()), "{line}");
        assert!(!is_network_uri(line), "{line}");
    }
    assert_eq!(commit("file://otherbox/etc"), Commit::Refused);
    assert_eq!(commit("   "), Commit::Nothing);
    assert_eq!(
        completion_dir("./user@east:/wo", CURRENT, HOME),
        "/fixture/current/user@east:"
    );
}
