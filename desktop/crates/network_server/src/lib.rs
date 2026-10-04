use crossbeam::channel::Sender;
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use protocol::{EngineCommand, EngineEvent, EngineSnapshot};
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, watch};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

pub struct WsServer {
    addr: SocketAddr,
    cmd_sender: Sender<EngineCommand>,
    events: broadcast::Sender<EngineEvent>,
    snapshot_provider: Option<std::sync::Arc<dyn Fn() -> EngineSnapshot + Send + Sync>>,
    shutdown: Option<watch::Receiver<bool>>,
}

impl WsServer {
    pub fn new(
        addr: SocketAddr,
        cmd_sender: Sender<EngineCommand>,
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
    cmd_sender: Sender<EngineCommand>,
    events: broadcast::Sender<EngineEvent>,
    snapshot_provider: Option<std::sync::Arc<dyn Fn() -> EngineSnapshot + Send + Sync>>,
) -> anyhow::Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (mut ws_sink, mut ws_source) = ws_stream.split();

    // Bring the client up to the current state.
    if let Some(provider) = snapshot_provider {
        for evt in (provider)().sync_events() {
            let msg = serde_json::to_string(&evt)?;
            ws_sink.send(Message::Text(msg.into())).await?;
        }
    }

    let mut event_rx = events.subscribe();

    loop {
        tokio::select! {
            // Incoming commands
            msg = ws_source.next() => {
                let Some(msg) = msg else { break; };
                let msg = msg?;
                if msg.is_text() {
                    let text = msg.to_text()?;
                    match serde_json::from_str::<EngineCommand>(text) {
                        Ok(cmd) => {
                            let _ = cmd_sender.send(cmd);
                        }
                        Err(e) => {
                            error!("Failed to parse command: {}", e);
                        }
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
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Drop lagged events; client can request a fresh snapshot.
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    Ok(())
}
