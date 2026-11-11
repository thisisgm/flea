use super::*;
use crate::ipc::Surface;
use flea::ipc::endpoint;

// The headless surface gui-ipc.sh drives, as the ipc-fixture example serves it, so a `--lib` build needs no example.
#[test]
fn ipc_fixture_child() {
    let Ok(path) = std::env::var("FLEA_GUI_IPC_FIXTURE_PATH") else {
        return;
    };
    let runtime = endpoint::runtime().expect("a private test runtime");
    assert!(
        runtime.join(".flea-test-sandbox").is_file(),
        "the fixture needs a marked runtime"
    );
    let server = Server::bind(&runtime).expect("test IPC server");
    while let Ok(event) = server.events.recv_blocking() {
        match event {
            Event::Call { request, answer } => {
                let surface = Surface {
                    path: &path,
                    total: 3,
                    cursor: 1,
                    selected: &[0, 2],
                };
                let _ = answer.try_send(surface.dispatch(request));
            }
            Event::Quit => break,
        }
    }
}
