use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use network_server::pairing::Pairing;
use network_server::{Identity, Listen, RemoteServer};
use protocol::{DisplayMode, EngineEvent, EngineSnapshot, FeedDisplay, FeedState, ProgramState};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, watch};

async fn next(socket: &mut (impl StreamExt<Item = Result<tokio_tungstenite::tungstenite::Message, tokio_tungstenite::tungstenite::Error>> + Unpin)) -> FeedState {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next()).await.unwrap().unwrap().unwrap();
        if message.is_text() {
            return serde_json::from_str(message.to_text().unwrap()).unwrap();
        }
    }
}

#[tokio::test]
async fn obs_reads_what_is_on_screen() {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let (_listen, listen_rx) = watch::channel(Listen { lan: false, feed: false, port });
    let (events, _) = broadcast::channel(16);
    let snapshot = EngineSnapshot { program_state: ProgramState { active_text: "Hello\nworld".into(), ..ProgramState::default() }, ..EngineSnapshot::default() };
    let (_shutdown, shutdown_rx) = watch::channel(false);
    let pairing = Arc::new(Pairing::load(std::env::temp_dir().join(format!("palto-feed-test-{port}.json"))));
    let identity = Identity { name: "Church PC".into(), version: "0.2.0".into() };
    let server = RemoteServer::new(listen_rx, Arc::new(|_| Ok(())), events.clone(), Arc::new(move || snapshot.clone()), pairing, identity, shutdown_rx);
    let status = server.status();
    tokio::spawn(server.run());
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut http = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    http.write_all(b"GET /api/check HTTP/1.1\r\nHost: x\r\n\r\n").await.unwrap();
    let mut body = String::new();
    http.read_to_string(&mut body).await.unwrap();
    let json: serde_json::Value = serde_json::from_str(body.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!((json["app"].as_str(), json["name"].as_str(), json["feed_versions"][0].as_u64()), (Some("palto"), Some("Church PC"), Some(1)));

    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/feed/v1")).await.unwrap();
    let first = next(&mut socket).await;
    assert_eq!((first.seq, first.lines.clone()), (1, vec!["Hello".to_string(), "world".to_string()]));
    assert_eq!(status.lock().unwrap().feed_clients.len(), 1);

    events.send(EngineEvent::ProgramChanged { program_state: ProgramState { active_text: "Hello\nworld".into(), ..ProgramState::default() }, version: 2 }).unwrap();
    events.send(EngineEvent::ProgramChanged { program_state: ProgramState { display: DisplayMode::Black, ..ProgramState::default() }, version: 3 }).unwrap();
    let second = next(&mut socket).await;
    assert_eq!((second.seq, second.display, second.text.as_str()), (2, FeedDisplay::Black, ""), "unchanged content is not sent again");

    drop(socket);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(status.lock().unwrap().feed_clients.is_empty());
}
