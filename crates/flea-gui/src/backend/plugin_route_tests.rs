use super::{Route, State};
use crate::backend::Control;

fn route(capacity: usize) -> (Route, async_channel::Receiver<(u64, String)>) {
    let (sender, receiver) = async_channel::bounded(capacity);
    (
        Route {
            state: Default::default(),
            sender,
        },
        receiver,
    )
}

fn send(route: &Route, line: &str) -> Result<bool, String> {
    route
        .intercept(&Control::Line(line.into()))
        .map_err(|failure| failure.detail)
}

fn state(route: &Route) -> (bool, u64) {
    let State { active, epoch } = *route.state.lock().unwrap();
    (active, epoch)
}

const PLACE: &str = r#"{"c":"list","path":"plugin://example.virtual-place/"}"#;
const WINDOW: &str = r#"{"c":"window","start":0,"count":5}"#;

#[test]
fn plugin_place_paste_copy_to_local_selection_validates_on_its_owner() {
    let (route, queued) = route(8);
    let validate = r#"{"c":"menuaction","op":"validate","id":71,"action":"copyTo","dest":"plugin://example.virtual-place/Photos"}"#;
    assert_eq!(
        send(&route, validate),
        Ok(false),
        "the local backend owns the source snapshot"
    );
    assert!(queued.try_recv().is_err());
    assert_eq!(send(&route, PLACE), Ok(true));
    queued.try_recv().unwrap();
    assert_eq!(
        send(&route, validate),
        Ok(true),
        "a plugin source validates in the place model"
    );
}

#[test]
fn a_plugin_list_takes_the_pane_and_a_local_list_gives_it_back() {
    let (route, queued) = route(8);
    assert_eq!(
        send(&route, WINDOW),
        Ok(false),
        "a local pane keeps its backend"
    );
    assert_eq!(send(&route, PLACE), Ok(true));
    assert_eq!(state(&route), (true, 1));
    assert_eq!(queued.try_recv().unwrap(), (1, PLACE.into()));
    assert_eq!(
        send(&route, WINDOW),
        Ok(true),
        "the place's own window goes to its owner"
    );
    assert_eq!(queued.try_recv().unwrap(), (1, WINDOW.into()));
    for passthrough in [
        r#"{"c":"clipGet","id":1}"#,
        r#"{"c":"plugins","op":"list"}"#,
    ] {
        assert_eq!(send(&route, passthrough), Ok(false), "{passthrough}");
    }
    assert_eq!(send(&route, r#"{"c":"list","path":"/home/u"}"#), Ok(false));
    assert_eq!(state(&route), (false, 2));
    assert_eq!(send(&route, WINDOW), Ok(false));
    assert!(
        queued.try_recv().is_err(),
        "nothing local reached the owner"
    );
    assert_eq!(send(&route, PLACE), Ok(true));
    assert_eq!(
        send(&route, r#"{"c":"listpaths","paths":["/a"]}"#),
        Ok(false)
    );
    assert_eq!(state(&route), (false, 4));
}

#[test]
fn a_plugin_path_or_destination_reaches_the_owner_from_a_local_pane() {
    let (route, queued) = route(8);
    for line in [
        r#"{"c":"transfer","op":"copy","paths":["/a/f"],"dest":"plugin://example.virtual-place/in"}"#,
        r#"{"c":"transfer","op":"copy","paths":["/a/f","plugin://example.virtual-place/g"],"dest":"/b"}"#,
    ] {
        assert_eq!(send(&route, line), Ok(true), "{line}");
        assert_eq!(queued.try_recv().unwrap(), (0, line.into()));
    }
    let local = r#"{"c":"transfer","op":"copy","paths":["/a/f"],"dest":"/b"}"#;
    assert_eq!(send(&route, local), Ok(false));
    assert_eq!(state(&route), (false, 0));
}

#[test]
fn a_refused_list_leaves_the_route_where_it_was() {
    let (route, queued) = route(1);
    assert_eq!(
        send(
            &route,
            r#"{"c":"transfer","paths":["plugin://a.b/x"],"dest":"/b"}"#
        ),
        Ok(true)
    );
    assert_eq!(
        send(&route, PLACE),
        Err("Plugin request queue is full.".into())
    );
    assert_eq!(state(&route), (false, 0), "the full queue moved nothing");
    assert_eq!(
        send(&route, WINDOW),
        Ok(false),
        "the local listing keeps its backend"
    );
    drop(queued);
    assert_eq!(
        send(&route, PLACE),
        Err("Plugin place owner has stopped.".into())
    );
    assert_eq!(state(&route), (false, 0));
}
