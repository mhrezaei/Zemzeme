//! A one-shot fake HTTP server for tests, standing in for OpenRouter so the
//! request/response handling can be tested without network or credits.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;
use std::time::Duration;

pub struct Request {
    pub head: String,
    pub body: serde_json::Value,
}

/// Answers the first request with `status` and `body` after `delay`.
/// Returns the base URL and a handle that yields the request it received.
pub fn serve(status: u16, body: &str, delay: Duration) -> (String, JoinHandle<Request>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let body = body.to_string();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut head = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            head.push_str(&line);
        }
        let len = head
            .lines()
            .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().to_string()))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let mut raw = vec![0; len];
        reader.read_exact(&mut raw).unwrap();
        std::thread::sleep(delay);
        let reply = format!(
            "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        // The client may have given up already (timeout tests).
        let _ = stream.write_all(reply.as_bytes());
        Request { head, body: serde_json::from_slice(&raw).unwrap_or_default() }
    });
    (url, handle)
}

/// An address with nothing listening on it.
pub fn closed_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}
