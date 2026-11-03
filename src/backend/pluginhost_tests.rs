use flea::jsondoc::{self, Json};
use flea::ui_logic_st2::wire::{self, Reply};
use std::io::{BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(5);

struct Output {
    buffer: Vec<u8>,
    lines: mpsc::Sender<String>,
    publish: mpsc::Receiver<()>,
}

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(bytes);
        while let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
            let line = String::from_utf8(self.buffer.drain(..=end).collect()).unwrap();
            let first = id(&line) == 0;
            self.lines.send(line).unwrap();
            if first {
                self.publish.recv_timeout(WAIT).unwrap();
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn id(line: &str) -> u64 {
    jsondoc::parse(line)
        .unwrap()
        .get("id")
        .unwrap()
        .as_f64()
        .unwrap() as u64
}

fn send(input: &mut UnixStream, id: u64, op: &str) {
    writeln!(
        input,
        r#"{{"c":"plugins","op":"{op}","id":{id},"plugin":"example.busy"}}"#
    )
    .unwrap();
}

#[test]
fn plugin_host_saturation_refuses_each_request_and_releases_before_terminal_publication() {
    let (mut input, reader) = UnixStream::pair().unwrap();
    let (lines, received) = mpsc::channel();
    let (publish, publication) = mpsc::channel();
    let (started, starts) = mpsc::channel();
    let gates = Arc::new((Mutex::new((false, false)), Condvar::new()));
    let worker_gates = gates.clone();
    let host = std::thread::spawn(move || {
        super::serve(
            BufReader::new(reader),
            Output {
                buffer: Vec::new(),
                lines,
                publish: publication,
            },
            move |line, _| {
                let id = id(line);
                started.send(id).unwrap();
                if id < 16 {
                    let (held, changed) = &*worker_gates;
                    let (open, timeout) = changed
                        .wait_timeout_while(held.lock().unwrap(), WAIT, |open| {
                            if id == 0 {
                                !open.0
                            } else {
                                !open.1
                            }
                        })
                        .unwrap();
                    assert!(!timeout.timed_out(), "request {id} was never released");
                    drop(open);
                }
                vec![format!(r#"{{"t":"pluginruntime","id":{id},"ok":true}}"#)]
            },
        )
    });
    for id in 0..16 {
        send(&mut input, id, "runtime");
    }
    let mut accepted: Vec<_> = (0..16)
        .map(|_| starts.recv_timeout(WAIT).unwrap())
        .collect();
    accepted.sort();
    assert_eq!(accepted, (0..16).collect::<Vec<_>>());
    for (id, op) in [(16, "runtime"), (17, "list"), (18, "set")] {
        send(&mut input, id, op);
        let reply = received
            .recv_timeout(WAIT)
            .expect("saturation must answer without ending the owner");
        assert_eq!(self::id(&reply), id);
        if op == "runtime" {
            let reply = jsondoc::parse(&reply).unwrap();
            assert_eq!(reply.get("t").and_then(Json::as_str), Some("pluginruntime"));
            assert_eq!(
                reply.get("plugin").and_then(Json::as_str),
                Some("example.busy")
            );
            assert_eq!(reply.get("ok").and_then(Json::as_bool), Some(false));
            assert_eq!(reply.get("code").and_then(Json::as_str), Some("busy"));
        } else {
            let Reply::Outcome(reply) = wire::parse(&reply).unwrap() else {
                panic!("expected refusal")
            };
            assert_eq!(reply.op, op);
            assert_eq!(reply.error.unwrap().code, "write");
        }
    }
    gates.0.lock().unwrap().0 = true;
    gates.1.notify_all();
    assert_eq!(id(&received.recv_timeout(WAIT).unwrap()), 0);
    send(&mut input, 19, "runtime");
    assert_eq!(
        starts
            .recv_timeout(WAIT)
            .expect("terminal publication must already free its slot"),
        19
    );
    publish.send(()).unwrap();
    gates.0.lock().unwrap().1 = true;
    gates.1.notify_all();
    let mut completed: Vec<_> = (0..16)
        .map(|_| id(&received.recv_timeout(WAIT).unwrap()))
        .collect();
    completed.sort();
    assert_eq!(completed, (1..16).chain([19]).collect::<Vec<_>>());
    drop(input);
    for worker in host.join().unwrap() {
        worker.join().unwrap();
    }
}

#[test]
fn plugin_host_accepts_the_maximum_payload_before_its_framing_newline() {
    let mut payload = r#"{"c":"plugins","op":"list","id":1}"#.to_string();
    payload.extend(
        std::iter::repeat(' ').take(flea::plugin_runtime::protocol::MAX_MESSAGE - payload.len()),
    );
    payload.push('\n');
    let (sent, received) = mpsc::channel();
    let workers = super::serve(std::io::Cursor::new(payload), Vec::new(), move |line, _| {
        sent.send(line.len()).unwrap();
        Vec::new()
    });
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(
        received.recv_timeout(WAIT).unwrap(),
        flea::plugin_runtime::protocol::MAX_MESSAGE
    );
}
