//! Headless surface for exercising the production IPC server without a compositor.
use flea_gui::ipc::{
    server::{Event, Server},
    Surface,
};

fn main() {
    let runtime = flea::ipc::endpoint::runtime().expect("a private test runtime");
    assert!(
        runtime.join(".flea-test-sandbox").is_file(),
        "the fixture needs a marked runtime"
    );
    let path = std::env::args().nth(1).expect("fixture path");
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
