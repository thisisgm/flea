//! A single application-wide plugin owner, without listing or thumbnail machinery.
use flea::plugin_runtime::protocol::{self, MAX_MESSAGE};
use flea::ui_logic_st2::manifest::Invalid;
use std::io::{BufRead, Write};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

pub fn run() -> i32 {
    let workers = serve(
        std::io::BufReader::new(std::io::stdin()),
        std::io::stdout(),
        |line, progress| match super::plugins::Env::user() {
            Ok(env) => super::plugins::handle_stream(line, &env, progress),
            Err(error) => vec![super::plugins::refusal(line, Invalid::new("write", error))],
        },
    );
    super::pluginruntime::shutdown();
    for worker in workers {
        let _ = worker.join();
    }
    0
}

fn serve<R, W, F>(mut input: R, output: W, handle: F) -> Vec<std::thread::JoinHandle<()>>
where
    R: BufRead,
    W: Write + Send + 'static,
    F: Fn(&str, &mut dyn FnMut(String)) -> Vec<String> + Send + Sync + 'static,
{
    let stdout = Arc::new(Mutex::new(output));
    let handle = Arc::new(handle);
    let live = Arc::new(AtomicUsize::new(0));
    let mut workers = Vec::new();
    loop {
        let mut line = Vec::new();
        let read =
            std::io::Read::take(&mut input, MAX_MESSAGE as u64 + 2).read_until(b'\n', &mut line);
        match read {
            Ok(0) | Err(_) => break,
            _ => {}
        }
        if line.len() > MAX_MESSAGE + 1 || line.last() != Some(&b'\n') {
            break;
        }
        line.pop();
        let Ok(doc) = protocol::parse(&line) else {
            break;
        };
        if doc.get("c").and_then(flea::jsondoc::Json::as_str) != Some("plugins") {
            break;
        }
        let Ok(line) = String::from_utf8(line) else {
            break;
        };
        if live.load(Ordering::Acquire) >= 16 {
            let code = if doc.get("op").and_then(flea::jsondoc::Json::as_str) == Some("runtime") {
                "busy"
            } else {
                "write"
            };
            let reply = super::plugins::refusal(&line, Invalid::new(code, "plugin host busy"));
            let mut stdout = stdout.lock().unwrap_or_else(|p| p.into_inner());
            let _ = writeln!(stdout, "{reply}");
            let _ = stdout.flush();
            continue;
        }
        live.fetch_add(1, Ordering::AcqRel);
        let stdout = stdout.clone();
        let live = live.clone();
        let handle = handle.clone();
        workers.retain(|worker: &std::thread::JoinHandle<()>| !worker.is_finished());
        workers.push(std::thread::spawn(move || {
            let lines = handle(&line, &mut |line| {
                let mut stdout = stdout.lock().unwrap_or_else(|p| p.into_inner());
                let _ = writeln!(stdout, "{line}");
                let _ = stdout.flush();
            });
            let mut stdout = stdout.lock().unwrap_or_else(|p| p.into_inner());
            // Publish a terminal answer only after its replacement can take the freed slot.
            live.fetch_sub(1, Ordering::AcqRel);
            for line in lines {
                let _ = writeln!(stdout, "{line}");
            }
            let _ = stdout.flush();
        }));
    }
    workers
}

#[cfg(test)]
#[path = "pluginhost_tests.rs"]
mod tests;
