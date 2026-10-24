//! Remote control: the engine's commands and events over a WebSocket, plus
//! the phone remote's page, on one port. With LAN access off it listens on
//! this computer only; with it on, other devices must pair first (a code on
//! the operator screen, then a token), and the remote is advertised as
//! `palto.local`. Clients on this computer need no pairing unless they are
//! web pages from another site.

pub mod http;
pub mod mdns;
pub mod osc;
pub mod pairing;
mod throttle;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use protocol::{EngineCommand, EngineEvent, EngineSnapshot};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, watch};
use tokio_tungstenite::tungstenite::Message;

use pairing::Pairing;
use throttle::Throttle;

/// Delivers a client command to the engine, or explains why it was refused.
pub type CommandSink = Arc<dyn Fn(EngineCommand) -> Result<(), String> + Send + Sync>;
pub type SnapshotProvider = Arc<dyn Fn() -> EngineSnapshot + Send + Sync>;

/// Commands a client may send per second, and in one burst.
const COMMANDS_PER_SECOND: f64 = 15.0;
const COMMAND_BURST: f64 = 30.0;
/// How often a paired device's connection re-checks that it is still paired.
const RECHECK: Duration = Duration::from_secs(2);
/// After a failed bind, try again this often (the port may free up).
const REBIND: Duration = Duration::from_secs(5);

/// Where to listen. Changing it rebinds at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listen {
    /// All interfaces when true, else this computer only.
    pub lan: bool,
    pub port: u16,
}

impl Listen {
    pub fn addr(&self) -> SocketAddr {
        let ip = if self.lan { IpAddr::V4(Ipv4Addr::UNSPECIFIED) } else { IpAddr::V4(Ipv4Addr::LOCALHOST) };
        SocketAddr::new(ip, self.port)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerStatus {
    pub listening: Option<SocketAddr>,
    pub error: Option<String>,
    /// Open connections from other devices.
    pub remote_clients: usize,
}

pub struct RemoteServer {
    listen: watch::Receiver<Listen>,
    commands: CommandSink,
    events: broadcast::Sender<EngineEvent>,
    snapshot: SnapshotProvider,
    pairing: Arc<Pairing>,
    status: Arc<Mutex<ServerStatus>>,
    shutdown: watch::Receiver<bool>,
}

struct Shared {
    commands: CommandSink,
    events: broadcast::Sender<EngineEvent>,
    snapshot: SnapshotProvider,
    pairing: Arc<Pairing>,
    status: Arc<Mutex<ServerStatus>>,
    listen: watch::Receiver<Listen>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

impl RemoteServer {
    pub fn new(
        listen: watch::Receiver<Listen>,
        commands: CommandSink,
        events: broadcast::Sender<EngineEvent>,
        snapshot: SnapshotProvider,
        pairing: Arc<Pairing>,
        shutdown: watch::Receiver<bool>,
    ) -> Self {
        Self { listen, commands, events, snapshot, pairing, status: Arc::default(), shutdown }
    }

    /// Where the server listens and why it cannot, for the operator screen.
    pub fn status(&self) -> Arc<Mutex<ServerStatus>> {
        self.status.clone()
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let shared = Arc::new(Shared {
            commands: self.commands,
            events: self.events,
            snapshot: self.snapshot,
            pairing: self.pairing,
            status: self.status,
            listen: self.listen.clone(),
        });
        let mut listen = self.listen;
        let mut shutdown = self.shutdown;
        loop {
            let target = *listen.borrow_and_update();
            let listener = match TcpListener::bind(target.addr()).await {
                Ok(listener) => {
                    info!("Remote server listening on {}", target.addr());
                    let mut status = shared.status.lock().expect("status poisoned");
                    status.listening = Some(target.addr());
                    status.error = None;
                    Some(listener)
                }
                Err(e) => {
                    error!("Network server failed: cannot listen on {}: {e}", target.addr());
                    let mut status = shared.status.lock().expect("status poisoned");
                    status.listening = None;
                    status.error = Some(format!("Port {} is not available ({e}). Choose another port.", target.port));
                    None
                }
            };
            loop {
                tokio::select! {
                    _ = shutdown.changed() => return Ok(()),
                    changed = listen.changed() => {
                        if changed.is_err() {
                            return Ok(());
                        }
                        if *listen.borrow() != target {
                            break;
                        }
                    }
                    _ = tokio::time::sleep(REBIND), if listener.is_none() => break,
                    accepted = async { listener.as_ref().expect("guarded").accept().await }, if listener.is_some() => {
                        match accepted {
                            Ok((stream, peer)) => {
                                let shared = shared.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle(stream, peer, shared).await {
                                        log::debug!("Remote connection from {peer}: {e:#}");
                                    }
                                });
                            }
                            Err(e) => warn!("Accepting a remote connection: {e}"),
                        }
                    }
                }
            }
        }
    }
}

/// Who a connection belongs to.
enum Client {
    /// A program on this computer.
    Local,
    /// A paired device, by id.
    Device(String),
}

/// A page from another site may not drive the outputs just because the
/// browser runs on this computer.
fn local_origin(origin: Option<&str>) -> bool {
    let Some(origin) = origin else { return true };
    let host = origin.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("");
    let host = host.rsplit_once(':').filter(|(h, port)| !h.is_empty() && port.chars().all(|c| c.is_ascii_digit())).map_or(host, |(h, _)| h);
    matches!(host, "localhost" | "127.0.0.1" | "[::1]") || host.eq_ignore_ascii_case(mdns::HOSTNAME)
}

async fn handle(stream: TcpStream, peer: SocketAddr, shared: Arc<Shared>) -> anyhow::Result<()> {
    let request = http::peek_request(&stream).await?;
    if !request.is_websocket() {
        return http::serve(stream, request, &shared.pairing, now_ms).await;
    }
    let client = if peer.ip().is_loopback() && local_origin(request.header("origin")) {
        Some(Client::Local)
    } else {
        request.param("token").and_then(|token| shared.pairing.authorize(token, now_ms())).map(|device| Client::Device(device.id))
    };
    let Some(client) = client else {
        use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
        let refuse = |_: &Request, _: Response| -> Result<Response, ErrorResponse> {
            let mut response = ErrorResponse::new(Some("Pair this device first".into()));
            *response.status_mut() = tokio_tungstenite::tungstenite::http::StatusCode::UNAUTHORIZED;
            Err(response)
        };
        let _ = tokio_tungstenite::accept_hdr_async(stream, refuse).await;
        warn!("Refused an unpaired remote connection from {peer}");
        return Ok(());
    };
    let remote = matches!(client, Client::Device(_));
    if remote {
        shared.status.lock().expect("status poisoned").remote_clients += 1;
        info!("Remote device connected from {peer}");
    }
    let result = connection(tokio_tungstenite::accept_async(stream).await?, &client, &shared).await;
    if remote {
        shared.status.lock().expect("status poisoned").remote_clients -= 1;
        info!("Remote device at {peer} disconnected");
    }
    result
}

async fn connection(socket: tokio_tungstenite::WebSocketStream<TcpStream>, client: &Client, shared: &Shared) -> anyhow::Result<()> {
    let (mut sink, mut source) = socket.split();
    // Subscribe before reading the snapshot so no change falls between them.
    // Events that predate the snapshot carry older versions/revisions, which
    // clients discard.
    let mut events = shared.events.subscribe();
    let sync = || (shared.snapshot)().sync_events().to_vec();
    for event in sync() {
        sink.send(Message::Text(serde_json::to_string(&event)?.into())).await?;
    }
    let mut throttle = Throttle::new(COMMANDS_PER_SECOND, COMMAND_BURST, Instant::now());
    let mut listen = shared.listen.clone();
    let mut recheck = tokio::time::interval(RECHECK);
    loop {
        tokio::select! {
            message = source.next() => {
                let Some(message) = message else { break };
                let message = message?;
                if !message.is_text() {
                    continue;
                }
                let refusal = if !throttle.allow(Instant::now()) {
                    Some("Too many commands at once; slow down".to_string())
                } else {
                    match serde_json::from_str::<EngineCommand>(message.to_text()?) {
                        Ok(command) => (shared.commands)(command).err(),
                        Err(e) => Some(format!("Invalid command: {e}")),
                    }
                };
                if let Some(message) = refusal {
                    warn!("Refused WebSocket command: {message}");
                    let event = EngineEvent::Error { source: "remote".into(), message, recoverable: true };
                    sink.send(Message::Text(serde_json::to_string(&event)?.into())).await?;
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if sink.send(Message::Text(serde_json::to_string(&event)?.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        // Every domain event is complete state, so a fresh
                        // snapshot fully replaces what was skipped.
                        warn!("WebSocket client lagged by {skipped} events; resynchronizing");
                        for event in sync() {
                            sink.send(Message::Text(serde_json::to_string(&event)?.into())).await?;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Other devices lose access when LAN access is turned off or
            // their pairing is removed.
            changed = listen.changed(), if matches!(client, Client::Device(_)) => {
                if changed.is_err() || !listen.borrow().lan {
                    break;
                }
            }
            _ = recheck.tick(), if matches!(client, Client::Device(_)) => {
                if let Client::Device(id) = client {
                    if !shared.pairing.is_paired(id) {
                        break;
                    }
                }
            }
        }
    }
    let _ = sink.close().await;
    Ok(())
}

/// `text` as a QR code in SVG, dark on light, for the operator screen.
pub fn qr_svg(text: &str) -> Option<String> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    Some(code.render::<qrcode::render::svg::Color>().min_dimensions(200, 200).quiet_zone(true).build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pages_from_this_computer_count_as_local() {
        assert!(local_origin(None), "programs send no origin");
        assert!(local_origin(Some("http://127.0.0.1:8080")));
        assert!(local_origin(Some("http://localhost:5174")));
        assert!(local_origin(Some("http://palto.local:8080")));
        assert!(!local_origin(Some("https://example.com")));
        assert!(!local_origin(Some("http://127.0.0.1.evil.example")));
        assert!(!local_origin(Some("null")));
    }

    #[test]
    fn a_qr_code_renders_as_svg() {
        assert!(qr_svg("http://192.168.1.20:8080/").unwrap().starts_with("<?xml"));
    }
}
