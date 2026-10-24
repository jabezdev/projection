//! The little HTTP the remote needs, on the same port as the WebSocket: the
//! phone remote's files, pairing, and a token check. Requests are read with
//! a size and time limit; anything else is a 404.

use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::pairing::Pairing;

const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 4 * 1024;
/// A client that sends its request slower than this is dropped.
pub const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// The phone remote, compiled in.
const FILES: &[(&str, &str, &str)] = &[
    ("/", "text/html; charset=utf-8", include_str!("../remote/index.html")),
    ("/remote.js", "text/javascript; charset=utf-8", include_str!("../remote/remote.js")),
    ("/remote.css", "text/css; charset=utf-8", include_str!("../remote/remote.css")),
    ("/manifest.webmanifest", "application/manifest+json", include_str!("../remote/manifest.webmanifest")),
    ("/icon.svg", "image/svg+xml", include_str!("../remote/icon.svg")),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    /// Without the query.
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    /// Bytes of the request line and headers, including the blank line.
    pub head_len: usize,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn is_websocket(&self) -> bool {
        self.header("upgrade").is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
    }

    /// A query parameter, percent-decoding not needed for tokens.
    pub fn param(&self, name: &str) -> Option<&str> {
        self.query.split('&').filter_map(|pair| pair.split_once('=')).find(|(k, _)| *k == name).map(|(_, v)| v)
    }
}

/// Parses a request head; `None` until `bytes` holds all of it.
pub fn parse(bytes: &[u8]) -> Option<Result<Request, &'static str>> {
    let end = bytes.windows(4).position(|w| w == b"\r\n\r\n")? + 4;
    let Ok(text) = std::str::from_utf8(&bytes[..end]) else { return Some(Err("not text")) };
    let mut lines = text.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split(' ');
    let (Some(method), Some(target), Some(version)) = (first.next(), first.next(), first.next()) else {
        return Some(Err("bad request line"));
    };
    if !version.starts_with("HTTP/1.") {
        return Some(Err("unsupported HTTP version"));
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let headers = lines
        .filter(|line| !line.is_empty())
        .filter_map(|line| line.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    Some(Ok(Request { method: method.to_string(), path: path.to_string(), query: query.to_string(), headers, head_len: end }))
}

/// Waits until the request head has arrived, without consuming it, so a
/// WebSocket handshake can still read it.
pub async fn peek_request(stream: &TcpStream) -> anyhow::Result<Request> {
    let deadline = Instant::now() + READ_TIMEOUT;
    let mut buffer = vec![0u8; MAX_HEAD];
    loop {
        let read = tokio::time::timeout_at(deadline.into(), stream.peek(&mut buffer)).await??;
        anyhow::ensure!(read > 0, "connection closed");
        if let Some(request) = parse(&buffer[..read]) {
            return request.map_err(|e| anyhow::anyhow!(e));
        }
        anyhow::ensure!(read < MAX_HEAD, "request head too large");
        anyhow::ensure!(Instant::now() < deadline, "request too slow");
        // Peek returns at once while data is waiting; give the rest time to arrive.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) -> anyhow::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.shutdown().await?;
    Ok(())
}

fn json_error(message: &str) -> Vec<u8> {
    serde_json::json!({ "error": message }).to_string().into_bytes()
}

#[derive(serde::Deserialize)]
struct PairRequest {
    code: String,
    #[serde(default)]
    name: String,
}

/// Serves one plain HTTP request (the head was peeked, not read).
pub async fn serve(mut stream: TcpStream, request: Request, pairing: &Pairing, now_ms: impl Fn() -> i64) -> anyhow::Result<()> {
    let mut head = vec![0u8; request.head_len];
    tokio::time::timeout(READ_TIMEOUT, stream.read_exact(&mut head)).await??;
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", path) => {
            if path == "/api/check" {
                // Tells the remote whether its saved token is still paired.
                let known = request.param("token").is_some_and(|token| pairing.authorize(token, now_ms()).is_some());
                return if known {
                    respond(&mut stream, "200 OK", "application/json", b"{\"paired\":true}").await
                } else {
                    respond(&mut stream, "401 Unauthorized", "application/json", b"{\"paired\":false}").await
                };
            }
            match FILES.iter().find(|(p, ..)| *p == path || (*p == "/" && path == "/index.html")) {
                Some((_, content_type, body)) => respond(&mut stream, "200 OK", content_type, body.as_bytes()).await,
                None => respond(&mut stream, "404 Not Found", "text/plain", b"Not found").await,
            }
        }
        ("POST", "/api/pair") => {
            let length: usize = request.header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
            if length > MAX_BODY {
                return respond(&mut stream, "413 Payload Too Large", "application/json", &json_error("Request too large")).await;
            }
            let mut body = vec![0u8; length];
            tokio::time::timeout(READ_TIMEOUT, stream.read_exact(&mut body)).await??;
            let Ok(pair) = serde_json::from_slice::<PairRequest>(&body) else {
                return respond(&mut stream, "400 Bad Request", "application/json", &json_error("Send a code")).await;
            };
            match pairing.pair(&pair.code, &pair.name, Instant::now(), now_ms()) {
                Ok(token) => respond(&mut stream, "200 OK", "application/json", serde_json::json!({ "token": token }).to_string().as_bytes()).await,
                Err(e) => respond(&mut stream, "403 Forbidden", "application/json", &json_error(&e.to_string())).await,
            }
        }
        _ => respond(&mut stream, "405 Method Not Allowed", "text/plain", b"Method not allowed").await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_websocket_upgrade_with_its_token() {
        let raw = b"GET /ws?token=abc&x=1 HTTP/1.1\r\nHost: palto.local:8080\r\nUpgrade: websocket\r\nOrigin: http://palto.local:8080\r\n\r\nrest";
        let request = parse(raw).unwrap().unwrap();
        assert_eq!((request.method.as_str(), request.path.as_str()), ("GET", "/ws"));
        assert_eq!(request.param("token"), Some("abc"));
        assert!(request.is_websocket());
        assert_eq!(request.header("origin"), Some("http://palto.local:8080"));
        assert_eq!(request.head_len, raw.len() - 4);
        assert!(parse(b"GET / HTTP/1.1\r\nHost: x\r\n").is_none(), "incomplete heads wait for more");
        assert!(parse(b"NONSENSE\r\n\r\n").unwrap().is_err());
    }
}
