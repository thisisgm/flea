//! A single application-wide plugin owner, without listing or thumbnail machinery.
use flea::plugin_runtime::protocol::{self, MAX_MESSAGE};
use std::io::{BufRead, Write};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

pub fn run() -> i32 {
    let stdout = Arc::new(Mutex::new(std::io::stdout()));
    let live = Arc::new(AtomicUsize::new(0));
    let mut workers = Vec::new();
    let mut input = std::io::BufReader::new(std::io::stdin());
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
            break;
        }
        live.fetch_add(1, Ordering::AcqRel);
        let stdout = stdout.clone();
        let live = live.clone();
        workers.retain(|worker: &std::thread::JoinHandle<()>| !worker.is_finished());
        workers.push(std::thread::spawn(move || {
            if let Ok(env) = super::plugins::Env::user() {
                let lines = super::plugins::handle_stream(&line, &env, &mut |line| {
                    let mut stdout = stdout.lock().unwrap_or_else(|p| p.into_inner());
                    let _ = writeln!(stdout, "{line}");
                    let _ = stdout.flush();
                });
                let mut stdout = stdout.lock().unwrap_or_else(|p| p.into_inner());
                for line in lines {
                    let _ = writeln!(stdout, "{line}");
                }
                let _ = stdout.flush();
            }
            live.fetch_sub(1, Ordering::AcqRel);
        }));
    }
    super::pluginruntime::shutdown();
    for worker in workers {
        let _ = worker.join();
    }
    0
}
