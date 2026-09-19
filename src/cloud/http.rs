// A bounded HTTP/1.0 exchange on a private Unix socket; never a TCP connection or redirect.
use crate::jsondoc::{self, Json};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

const LIMIT: usize = 2 * 1024 * 1024;

pub(super) fn post(socket: &Path, method: &str, body: &str) -> Result<Json, &'static str> {
    if !matches!(method, "vfs/stats" | "vfs/queue" | "vfs/list" | "core/stats") {
        return Err("Unsupported status method");
    }
    let mut stream = UnixStream::connect(socket).map_err(|_| "Cannot connect to status socket")?;
    let deadline = Instant::now() + Duration::from_millis(1200);
    stream.set_write_timeout(Some(Duration::from_millis(1200))).map_err(|_| "Cannot bound status request")?;
    let request = format!("POST /{} HTTP/1.0\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", method, body.len(), body);
    stream.write_all(request.as_bytes()).map_err(|_| "Status request failed")?;
    let mut response = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let remaining = deadline.checked_duration_since(Instant::now()).ok_or("Status query timed out")?;
        stream.set_read_timeout(Some(remaining)).map_err(|_| "Cannot bound status response")?;
        let n = stream.read(&mut buffer).map_err(|_| "Status response timed out or disconnected")?;
        if n == 0 { break; }
        if response.len() + n > LIMIT { return Err("Status response is too large"); }
        response.extend_from_slice(&buffer[..n]);
    }
    decode(&response)
}

fn decode(response: &[u8]) -> Result<Json, &'static str> {
    let cut = response.windows(4).position(|w| w == b"\r\n\r\n").filter(|cut| *cut <= 8192)
        .ok_or("Invalid status response headers")?;
    let head = std::str::from_utf8(&response[..cut]).map_err(|_| "Invalid status response headers")?;
    let mut lines = head.split("\r\n");
    let status: Vec<_> = lines.next().unwrap_or_default().split_whitespace().collect();
    if status.len() < 2 || !matches!(status[0], "HTTP/1.0" | "HTTP/1.1") || status[1] != "200" {
        return Err("Status endpoint refused the request");
    }
    let body = &response[cut + 4..];
    for line in lines {
        let (key, value) = line.split_once(':').ok_or("Invalid status header")?;
        if key.eq_ignore_ascii_case("transfer-encoding") { return Err("Unexpected status transfer encoding"); }
        if key.eq_ignore_ascii_case("content-length") && value.trim().parse::<usize>().ok() != Some(body.len()) {
            return Err("Truncated status response");
        }
    }
    let body = std::str::from_utf8(body).map_err(|_| "Invalid status JSON encoding")?;
    jsondoc::parse(body).map_err(|_| "Invalid status JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_disconnect_oversize_and_timeout_are_bounded_failures() {
        use std::os::unix::net::UnixListener;
        for behavior in ["disconnect", "oversize", "timeout"] {
            let d = crate::backend::testdir::TestDir::new("cloud-http");
            let path = d.join("rc.sock");
            let listener = UnixListener::bind(&path).unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 1024];
                let _ = stream.read(&mut request);
                match behavior {
                    "oversize" => { let _ = stream.write_all(&vec![b'x'; LIMIT + 1]); }
                    "timeout" => std::thread::sleep(Duration::from_millis(1400)),
                    _ => {}
                }
            });
            let started = Instant::now();
            assert!(post(&path, "vfs/stats", "{}").is_err(), "{}", behavior);
            assert!(started.elapsed() < Duration::from_secs(3));
            server.join().unwrap();
        }
    }
    #[test]
    fn errors_redirects_truncated_and_chunked_responses_never_become_idle() {
        assert!(decode(b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\n{}").is_ok());
        assert!(decode(b"HTTP/1.0 200 OK\r\n\r\n{}").is_ok());
        for wire in [b"HTTP/1.0 403 Forbidden\r\n\r\n{}".as_slice(),
            b"HTTP/1.0 302 Found\r\nLocation: http://elsewhere\r\n\r\n{}",
            b"HTTP/1.0 200 OK\r\nContent-Length: 99\r\n\r\n{}",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n"] {
            assert!(decode(wire).is_err());
        }
    }
}
