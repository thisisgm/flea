use super::{
    endpoint,
    wire::{self, Reply, Request},
};
use std::io::{self, BufReader};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, PartialEq)]
pub struct Options {
    pub pid: Option<u32>,
    pub request: Request,
}

pub fn parse(arguments: &[String]) -> Result<Options, Reply> {
    let mut pid = None;
    let mut any = false;
    let mut at = 0;
    while let Some(argument) = arguments.get(at) {
        match argument.as_str() {
            "--pid" | "--id" | "-i" => {
                at += 1;
                let value = arguments
                    .get(at)
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|pid| *pid > 0 && *pid <= i32::MAX as u32)
                    .ok_or_else(|| Reply::error(2, "--pid needs a positive process id"))?;
                if pid.replace(value).is_some() || any {
                    return Err(Reply::error(2, "choose only one IPC instance selector"));
                }
            }
            "--any" => {
                if any || pid.is_some() {
                    return Err(Reply::error(2, "choose only one IPC instance selector"));
                }
                any = true;
            }
            "call" | "show" => break,
            _ => {
                return Err(Reply::error(
                    2,
                    "usage: flea ipc [--pid N|--any] call <target> <function> [args...] | show",
                ))
            }
        }
        at += 1;
    }
    let request = match arguments.get(at).map(String::as_str) {
        Some("show") if arguments.len() == at + 1 => Request::Show,
        Some("call") => {
            let target = arguments
                .get(at + 1)
                .ok_or_else(|| Reply::value("Target required to send message."))?;
            let function = arguments
                .get(at + 2)
                .ok_or_else(|| Reply::value("Function required to send message."))?;
            Request::Call {
                target: target.clone(),
                function: function.clone(),
                arguments: arguments[at + 3..].to_vec(),
            }
        }
        _ => {
            return Err(Reply::error(
                2,
                "usage: flea ipc [--pid N|--any] call <target> <function> [args...] | show",
            ))
        }
    };
    Ok(Options { pid, request })
}

pub fn exchange(path: &Path, request: &Request) -> io::Result<Reply> {
    let line = request.encode();
    if line.len() > wire::REQUEST_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "IPC line is too long",
        ));
    }
    let mut socket = super::connect::with_timeout(path, Duration::from_secs(5))?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    socket.set_write_timeout(Some(Duration::from_secs(5)))?;
    wire::write_line(&mut socket, &line)?;
    let line = wire::read_line(&mut BufReader::new(socket), wire::REPLY_LIMIT)?;
    Reply::decode(&line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn execute(root: &Path, options: &Options) -> Reply {
    // A chooser is reached only by its pid, never by the default route.
    let sockets = match endpoint::instances(root).and_then(|mut sockets| {
        if options.pid.is_some() {
            sockets.extend(endpoint::pickers(root)?);
        }
        Ok(sockets)
    }) {
        Ok(sockets) => sockets,
        Err(error) => return Reply::error(255, format!("flea ipc: {error}")),
    };
    let mut matching = sockets
        .iter()
        .filter(|(pid, _)| options.pid.is_none_or(|wanted| wanted == *pid));
    for (_, path) in &mut matching {
        match exchange(path, &options.request) {
            Ok(reply) => return reply,
            Err(error)
                if error.kind() == io::ErrorKind::ConnectionRefused
                    || error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Reply::error(255, format!("flea ipc: {error}")),
        }
    }
    match options.pid {
        Some(pid) => Reply::value(format!("No instance found for pid {pid}")).with_code(255),
        None => Reply::error(255, "No running instances for flea-gui"),
    }
}

impl Reply {
    fn with_code(mut self, code: i32) -> Self {
        self.code = code;
        self
    }
}

pub fn run(arguments: &[String]) -> i32 {
    let reply = match parse(arguments) {
        Err(reply) => reply,
        Ok(options) => match endpoint::runtime() {
            Ok(root) => execute(&root, &options),
            Err(error) => Reply::error(255, format!("flea ipc: {error}")),
        },
    };
    print!("{}", reply.stdout);
    eprint!("{}", reply.stderr);
    reply.code
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
