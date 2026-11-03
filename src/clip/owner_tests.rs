// Owner-side fakes for offer retirement and deadline writes.
use super::*;
use crate::clip::format;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use crate::clip::testutil::{advertise, expect, over, serve, watchdog, WATCHDOG_MS};

// The owner's setup round trip: globals, done, then everything up to the closing sync.
fn owner_hello(conn: &mut Conn) {
    let registry = expect(conn, DISPLAY, DISPLAY_GET_REGISTRY);
    let callback = expect(conn, DISPLAY, DISPLAY_SYNC);
    advertise(conn, registry, &[("wl_seat", 1), ("ext_data_control_manager_v1", 1)]);
    conn.send(callback, 0, &[], &[]).unwrap();
    loop {
        let event = conn.next_raw(WATCHDOG_MS).unwrap().expect("a request");
        if event.sender == DISPLAY && event.opcode == DISPLAY_SYNC {
            break;
        }
    }
    conn.send(8, 0, &[], &[]).unwrap();
}

fn start_owner(path: PathBuf, paths: Vec<String>) -> (std::thread::JoinHandle<()>, std::sync::mpsc::Receiver<()>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut conn = Conn::connect_to(&path).unwrap();
        crate::clip::control::handshake(&mut conn).unwrap();
        let mut owner = own_with(conn, "copy", &paths, "ab12cd34ab12cd34ab12cd34ab12cd34", None).unwrap();
        serve_owner(&mut owner).unwrap();
        let _ = tx.send(());
    });
    (worker, rx)
}

#[test]
fn superseded_device_offers_are_destroyed() {
    let (_dir, listener, path) = serve("clip-owner-retire");
    let (worker, _done) = start_owner(path, vec!["/tmp/a.txt".to_string()]);
    let stream = crate::clip::testutil::accept(&listener, watchdog()).unwrap();
    let mut conn = over(stream);
    owner_hello(&mut conn);
    for id in [20u32, 21, 22] {
        let mut payload = Vec::new();
        wire::put_u32(&mut payload, id);
        conn.send(7, 0, &payload, &[]).unwrap();
    }
    let mut payload = Vec::new();
    wire::put_u32(&mut payload, 21);
    conn.send(7, 1, &payload, &[]).unwrap();
    // The two superseded offers are destroyed, in either order; the current one stays.
    let mut destroyed = Vec::new();
    for _ in 0..2 {
        let event = conn.next_raw(WATCHDOG_MS).unwrap().expect("a destroy");
        assert_eq!(event.opcode, 1);
        assert!(event.body.is_empty());
        destroyed.push(event.sender);
    }
    destroyed.sort_unstable();
    assert_eq!(destroyed, vec![20, 22]);
    conn.send(6, 1, &[], &[]).unwrap();
    worker.join().unwrap();
}

#[test]
fn a_reader_that_stops_reading_does_not_wedge_the_owner() {
    // A real pipe at 64 KiB and a payload near half a megabyte: a blocking write never returns.
    let paths: Vec<String> = (0..20000).map(|i| format!("/tmp/f{:05}.txt", i)).collect();
    let (_dir, listener, path) = serve("clip-owner-stuck");
    let (worker, done) = start_owner(path, paths);
    let stream = crate::clip::testutil::accept(&listener, watchdog()).unwrap();
    let mut conn = over(stream);
    owner_hello(&mut conn);
    // Asked for, never read, then replaced: only a write deadline lets the owner reach cancelled.
    extern "C" {
        fn pipe(fds: *mut std::os::raw::c_int) -> std::os::raw::c_int;
    }
    use std::os::fd::FromRawFd;
    let mut fds = [-1, -1];
    assert_eq!(unsafe { pipe(fds.as_mut_ptr()) }, 0);
    let mut payload = Vec::new();
    wire::put_string(&mut payload, format::URILIST);
    conn.send(6, 0, &payload, &[fds[1]]).unwrap();
    drop(unsafe { OwnedFd::from_raw_fd(fds[1]) });
    conn.send(6, 1, &[], &[]).unwrap();
    done.recv_timeout(watchdog()).expect("the owner completion hang guard expired");
    worker.join().unwrap();
    drop(unsafe { OwnedFd::from_raw_fd(fds[0]) });
}

#[test]
fn a_copy_with_content_offers_the_files_own_type_first_and_serves_its_bytes() {
    let (_dir, listener, path) = serve("clip-owner-content");
    let (tx, done) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut conn = Conn::connect_to(&path).unwrap();
        crate::clip::control::handshake(&mut conn).unwrap();
        let content = Some(("image/png".to_string(), b"\x89PNG bytes".to_vec()));
        let mut owner = own_with(conn, "copy", &["/tmp/shot.png".to_string()], "ab12cd34ab12cd34ab12cd34ab12cd34", content).unwrap();
        serve_owner(&mut owner).unwrap();
        let _ = tx.send(());
    });
    let stream = crate::clip::testutil::accept(&listener, watchdog()).unwrap();
    let mut conn = over(stream);
    let registry = expect(&mut conn, DISPLAY, DISPLAY_GET_REGISTRY);
    let callback = expect(&mut conn, DISPLAY, DISPLAY_SYNC);
    advertise(&mut conn, registry, &[("wl_seat", 1), ("ext_data_control_manager_v1", 1)]);
    conn.send(callback, 0, &[], &[]).unwrap();
    let mut offers = Vec::new();
    loop {
        let event = conn.next_raw(WATCHDOG_MS).unwrap().expect("a request");
        if event.sender == DISPLAY && event.opcode == DISPLAY_SYNC {
            break;
        }
        if event.sender == OWNER_SOURCE && event.opcode == SOURCE_OFFER {
            let mut at = 0;
            offers.push(wire::get_string(&event.body, &mut at).unwrap());
        }
    }
    conn.send(8, 0, &[], &[]).unwrap();
    assert_eq!(offers.first().map(String::as_str), Some("image/png"));
    assert!(offers.iter().any(|o| o == format::GNOME) && offers.iter().any(|o| o == format::URILIST), "the paths still ride beside the bytes: {:?}", offers);
    extern "C" {
        fn pipe(fds: *mut std::os::raw::c_int) -> std::os::raw::c_int;
    }
    use std::os::fd::FromRawFd;
    let mut fds = [-1, -1];
    assert_eq!(unsafe { pipe(fds.as_mut_ptr()) }, 0);
    let mut payload = Vec::new();
    wire::put_string(&mut payload, "image/png");
    conn.send(OWNER_SOURCE, SOURCE_SEND, &payload, &[fds[1]]).unwrap();
    drop(unsafe { OwnedFd::from_raw_fd(fds[1]) });
    let mut read = Vec::new();
    use std::io::Read;
    std::fs::File::from(unsafe { OwnedFd::from_raw_fd(fds[0]) }).read_to_end(&mut read).unwrap();
    assert_eq!(read, b"\x89PNG bytes");
    conn.send(OWNER_SOURCE, 1, &[], &[]).unwrap();
    done.recv_timeout(watchdog()).expect("the owner completion hang guard expired");
    worker.join().unwrap();
}
