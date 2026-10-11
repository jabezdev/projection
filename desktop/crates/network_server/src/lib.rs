use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use protocol::{EngineCommand, EngineEvent, EngineSnapshot};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, watch};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

/// Delivers a client command to the engine, or explains why it was refused.
pub type CommandSink = Arc<dyn Fn(EngineCommand) -> Result<(), String> + Send + Sync>;

pub struct WsServer {
    addr: SocketAddr,
    cmd_sender: CommandSink,
    events: broadcast::Sender<EngineEvent>,
    snapshot_provider: Option<std::sync::Arc<dyn Fn() -> EngineSnapshot + Send + Sync>>,
    shutdown: Option<watch::Receiver<bool>>,
}

impl WsServer {
    pub fn new(
        addr: SocketAddr,
        cmd_sender: CommandSink,
        events: broadcast::Sender<EngineEvent>,
    ) -> Self {
        Self {
            addr,
            cmd_sender,
            events,
            snapshot_provider: None,
            shutdown: None,
        }
    }

    /// Optional: provide an initial snapshot to send on connect.
    pub fn with_snapshot_provider(
        mut self,
        provider: std::sync::Arc<dyn Fn() -> EngineSnapshot + Send + Sync>,
    ) -> Self {
        self.snapshot_provider = Some(provider);
        self
    }

    pub fn with_shutdown(mut self, shutdown: watch::Receiver<bool>) -> Self {
        self.shutdown = Some(shutdown);
        self
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(self.addr).await?;
        info!("WebSocket server listening on {}", self.addr);

        let mut shutdown = self.shutdown;

        loop {
            tokio::select! {
                // Shutdown
                _ = async {
                    match &mut shutdown {
                        Some(rx) => {
                            let _ = rx.changed().await;
                        }
                        None => futures_util::future::pending().await,
                    }
                } => {
                    break;
                }

                // Accept new connection
                accept_res = listener.accept() => {
                    let (stream, _) = accept_res?;
                    let sender = self.cmd_sender.clone();
                    let events = self.events.clone();
                    let snapshot_provider = self.snapshot_provider.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, sender, events, snapshot_provider).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
            }
        }
        Ok(())
    }
}

async fn handle_connection(
    stream: TcpStream,
    cmd_sender: CommandSink,
    events: broadcast::Sender<EngineEvent>,
    snapshot_provider: Option<std::sync::Arc<dyn Fn() -> EngineSnapshot + Send + Sync>>,
) -> anyhow::Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (mut ws_sink, mut ws_source) = ws_stream.split();

    // Subscribe before reading the snapshot so no change falls between them.
    // Events that predate the snapshot carry older versions/revisions, which
    // clients discard.
    let mut event_rx = events.subscribe();
    let sync = |provider: &Option<std::sync::Arc<dyn Fn() -> EngineSnapshot + Send + Sync>>| {
        provider.as_ref().map(|p| p().sync_events().to_vec()).unwrap_or_default()
    };
    for evt in sync(&snapshot_provider) {
        ws_sink.send(Message::Text(serde_json::to_string(&evt)?.into())).await?;
    }

    loop {
        tokio::select! {
            // Incoming commands
            msg = ws_source.next() => {
                let Some(msg) = msg else { break; };
                let msg = msg?;
                if msg.is_text() {
                    let text = msg.to_text()?;
                    let refusal = match serde_json::from_str::<EngineCommand>(text) {
                        Ok(cmd) => (cmd_sender)(cmd).err(),
                        Err(e) => Some(format!("Invalid command: {e}")),
                    };
                    if let Some(message) = refusal {
                        warn!("Refused WebSocket command: {message}");
                        let evt = EngineEvent::Error { source: "remote".into(), message, recoverable: true };
                        ws_sink.send(Message::Text(serde_json::to_string(&evt)?.into())).await?;
                    }
                }
            }

            // Outgoing events
            evt = event_rx.recv() => {
                match evt {
                    Ok(evt) => {
                        let msg = serde_json::to_string(&evt)?;
                        if ws_sink.send(Message::Text(msg.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        // Every domain event is complete state, so a fresh
                        // snapshot fully replaces what was skipped.
                        warn!("WebSocket client lagged by {skipped} events; resynchronizing");
                        for evt in sync(&snapshot_provider) {
                            ws_sink.send(Message::Text(serde_json::to_string(&evt)?.into())).await?;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    Ok(())
}
