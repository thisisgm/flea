//! Socket work runs on the background executor, never on a frame or a pointer callback.
use super::carry_wire::Offer;
use flea::ipc::{
    client, endpoint,
    wire::{Reply, Request},
};
use std::path::{Path, PathBuf};

pub(super) fn request(function: &str, arguments: Vec<String>) -> Request {
    Request::Call {
        target: "flea".into(),
        function: function.into(),
        arguments,
    }
}

fn root() -> Option<PathBuf> {
    // In-process GUI tests never reach a live Flea; only the isolated fixture processes open sockets.
    #[cfg(test)]
    std::env::var_os("FLEA_CARRY_FIXTURE")?;
    endpoint::runtime().ok()
}

pub(super) fn call(pid: u32, function: &str, arguments: Vec<String>) -> Option<Reply> {
    let root = root()?;
    let path = endpoint::instances(&root)
        .ok()?
        .into_iter()
        .find(|(peer, _)| *peer == pid)?
        .1;
    client::exchange(&path, &request(function, arguments))
        .ok()
        .filter(|reply| reply.code == 0)
}

pub(super) fn fetch(pid: u32, token: &str) -> Option<Offer> {
    let reply = call(pid, "carryPeek", vec![token.into()])?;
    let offer = Offer::decode(&reply.stdout)?;
    (offer.pid == pid && (token.is_empty() || offer.token == token)).then_some(offer)
}

pub(super) fn discover() -> Vec<Offer> {
    let Some(peers) = root().and_then(|root| endpoint::instances(&root).ok()) else {
        return Vec::new();
    };
    peers
        .into_iter()
        .filter(|(pid, _)| *pid != std::process::id())
        .filter_map(|(pid, _)| fetch(pid, ""))
        .collect()
}

pub(super) fn broadcast(function: &str, arguments: Vec<String>) {
    let Some(root) = root() else {
        return;
    };
    let Ok(peers) = endpoint::instances(&root) else {
        return;
    };
    let request = request(function, arguments);
    // A stalled peer does not delay delivery to the other live windows.
    std::thread::scope(|scope| {
        for (pid, path) in &peers {
            if *pid != std::process::id() {
                let request = &request;
                scope.spawn(move || {
                    let _ = exchange(path, request);
                });
            }
        }
    });
}

fn exchange(path: &Path, request: &Request) -> std::io::Result<Reply> {
    client::exchange(path, request)
}
