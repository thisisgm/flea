//! Trash rows never ask for a thumbnail, as v0.3.12 draws the generic icon on every Trash row; the viewport arithmetic stays checked.
use super::host::Host;
use super::thumbs::viewport;
use super::view::{Signal, View};
use super::view_tests::{Backend, ROW};

fn image(index: usize) -> String {
    format!(r#"{{"uri":"trash:///image-{index}.png","original":"/original/image-{index}.png","directory":false,"deleted":"2026-10-05T09:00:00","icon":"image-x-generic","identity":"row-{index}","thumbnail":{{"path":"/fixture/Trash/files/image-{index}.png","identity":"backing-{index}","provider":"l1:{index}","mtime":5}}}}"#)
}

fn fixture() -> Vec<String> {
    let mut rows = vec![image(0), r#"{"uri":"trash:///notes.txt","original":"/original/notes.txt","directory":false,"icon":"text-x-generic","thumbnail":null}"#.to_string()];
    rows.extend((2..50).map(image));
    rows
}

/// Pumps the backend and fires every settle timer the view asked for, until both are quiet.
fn run(backend: &mut Backend, host: &mut Host) {
    loop {
        backend.pump(host);
        let settles: Vec<u64> = host.view.signals().into_iter().filter_map(|s| match s { Signal::Settle { generation, .. } => Some(generation), _ => None }).collect();
        if settles.is_empty() {
            return;
        }
        for generation in settles {
            host.view.settled(generation);
        }
    }
}

fn opened() -> (Backend, Host) {
    let mut backend = Backend::with(fixture());
    let mut host = Host::new(View::new(ROW));
    host.view.set_viewport(0., 124.);
    host.open("");
    run(&mut backend, &mut host);
    (backend, host)
}

#[test]
fn trash_rows_never_ask_for_a_thumbnail() {
    let (backend, host) = opened();
    assert!(backend.commands("trashthumb").is_empty(), "image rows keep the generic icon, so no row asks");
    assert_eq!(host.view.thumbs.file_for(0, host.view.policy), "", "no row ever holds an image source");
    assert_eq!(host.view.thumbs.file_for(2, host.view.policy), "", "far image rows draw no image either");
}

#[test]
fn trash_viewport_is_the_rows_in_view() {
    assert_eq!(viewport(0., ROW, 124., 0), None, "an empty Trash has no viewport");
    assert_eq!(viewport(15., ROW, 124., 49), Some((0, 4)), "a partly shown bottom row is in the viewport");
    assert_eq!(viewport(0., ROW, 124., 2), Some((0, 1)), "the viewport stops at the last row");
}
