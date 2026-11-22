use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use log::info;
use protocol::{EngineEvent, FeedState};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

use crate::Shared;

pub const MAX_CLIENTS: usize = 16;
const PING: Duration = Duration::from_secs(15);

struct Registered<'a> {
    shared: &'a Shared,
    peer: SocketAddr,
}

impl Drop for Registered<'_> {
    fn drop(&mut self) {
        let mut status = self.shared.status.lock().expect("status poisoned");
        if let Some(index) = status.feed_clients.iter().position(|p| *p == self.peer) {
            status.feed_clients.remove(index);
        }
        info!("OBS feed at {} disconnected", self.peer);
    }
}

fn register(shared: &Shared, peer: SocketAddr) -> Option<Registered<'_>> {
    let mut status = shared.status.lock().expect("status poisoned");
    if status.feed_clients.len() >= MAX_CLIENTS {
        return None;
    }
    status.feed_clients.push(peer);
    info!("OBS feed connected from {peer}");
    Some(Registered { shared, peer })
}

pub(crate) async fn serve(stream: TcpStream, peer: SocketAddr, shared: Arc<Shared>) -> anyhow::Result<()> {
    let Some(_registered) = register(&shared, peer) else {
        crate::refuse_websocket(stream, 503, "Too many OBS connections").await;
        return Ok(());
    };
    let socket = tokio_tungstenite::accept_async(stream).await?;
    let (mut sink, mut source) = socket.split();
    let mut events = shared.events.subscribe();
    let mut snapshot = (shared.snapshot)();
    let mut sent = FeedState { seq: 1, ..FeedState::from_snapshot(&snapshot) };
    sink.send(Message::Text(serde_json::to_string(&sent)?.into())).await?;
    let mut listen = shared.listen.clone();
    let mut ping = tokio::time::interval(PING);
    ping.tick().await;
    let local = peer.ip().is_loopback();
    loop {
        let changed = tokio::select! {
            message = source.next() => match message {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => false,
            },
            event = events.recv() => match event {
                Ok(EngineEvent::ServiceChanged { service, .. }) => {
                    snapshot.service = service;
                    true
                }
                Ok(EngineEvent::ProgramChanged { program_state, .. }) => {
                    snapshot.program_state = program_state;
                    true
                }
                Ok(_) => false,
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    snapshot = (shared.snapshot)();
                    true
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            changed = listen.changed(), if !local => {
                if changed.is_err() || !listen.borrow().feed {
                    break;
                }
                false
            }
            _ = ping.tick() => {
                sink.send(Message::Ping(Vec::new().into())).await?;
                false
            }
        };
        if !changed {
            continue;
        }
        let next = FeedState::from_snapshot(&snapshot);
        if !next.same_content(&sent) {
            sent = FeedState { seq: sent.seq + 1, ..next };
            sink.send(Message::Text(serde_json::to_string(&sent)?.into())).await?;
        }
    }
    let _ = sink.close().await;
    Ok(())
}
