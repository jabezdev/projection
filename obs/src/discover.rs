use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;
use std::time::Duration;

use futures_util::StreamExt;
use protocol::FEED_VERSION;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const DEFAULT_PORT: u16 = 8080;
const SERVICE: &str = "_palto._tcp.local.";
const BROWSE: Duration = Duration::from_millis(1500);
const PROBE_CONNECT: Duration = Duration::from_millis(300);
const PROBE_READ: Duration = Duration::from_secs(1);
const PROBES_AT_ONCE: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sharing {
    On,
    Off,
    PaltoTooOld,
    PluginTooOld,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub name: String,
    pub addr: SocketAddr,
    pub sharing: Sharing,
}

static LAST: Mutex<Vec<Found>> = Mutex::new(Vec::new());

pub fn last_found() -> Vec<Found> {
    LAST.lock().map(|f| f.clone()).unwrap_or_default()
}

pub fn sharing(versions: &[u32]) -> Sharing {
    if versions.contains(&FEED_VERSION) {
        Sharing::On
    } else if versions.iter().any(|v| *v > FEED_VERSION) {
        Sharing::PluginTooOld
    } else {
        Sharing::PaltoTooOld
    }
}

pub fn parse_address(text: &str) -> Option<SocketAddr> {
    let text = text.trim().trim_start_matches("http://").trim_end_matches('/');
    text.parse::<SocketAddr>().ok().or_else(|| text.parse::<IpAddr>().ok().map(|ip| SocketAddr::new(ip, DEFAULT_PORT)))
}

pub async fn check(addr: SocketAddr) -> Option<Found> {
    let mut stream = tokio::time::timeout(PROBE_CONNECT, TcpStream::connect(addr)).await.ok()?.ok()?;
    let request = format!("GET /api/check HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    let mut response = Vec::new();
    tokio::time::timeout(PROBE_READ, async {
        stream.write_all(request.as_bytes()).await?;
        stream.read_to_end(&mut response).await
    })
    .await
    .ok()?
    .ok()?;
    let response = String::from_utf8_lossy(&response);
    let body = response.split_once("\r\n\r\n")?.1;
    let json: serde_json::Value = serde_json::from_str(body).ok()?;
    if json["app"] != "palto" {
        return None;
    }
    let versions: Vec<u32> = json["feed_versions"].as_array().map(|v| v.iter().filter_map(|n| n.as_u64()).map(|n| n as u32).collect()).unwrap_or_default();
    let name = json["name"].as_str().filter(|n| !n.is_empty()).map_or_else(|| addr.ip().to_string(), String::from);
    Some(Found { name, addr, sharing: sharing(&versions) })
}

async fn browse() -> Vec<Found> {
    let Ok(daemon) = mdns_sd::ServiceDaemon::new() else { return Vec::new() };
    let Ok(receiver) = daemon.browse(SERVICE) else {
        let _ = daemon.shutdown();
        return Vec::new();
    };
    let own = own_addresses();
    let mut found: Vec<(String, Found)> = Vec::new();
    let deadline = tokio::time::Instant::now() + BROWSE;
    while let Ok(Ok(event)) = tokio::time::timeout_at(deadline, receiver.recv_async()).await {
        if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
            let Some(ip) = info.get_addresses_v4().into_iter().copied().min_by_key(|ip| rank(*ip, &own)) else { continue };
            let name = info.get_property_val_str("name").filter(|n| !n.is_empty()).map_or_else(|| ip.to_string(), String::from);
            let sharing = match info.get_property_val_str("feed").and_then(|v| v.parse::<u32>().ok()) {
                Some(0) => Sharing::Off,
                Some(version) => sharing(&[version]),
                None => Sharing::PaltoTooOld,
            };
            let fullname = info.get_fullname().to_string();
            let better = |f: &Found| matches!(f.addr.ip(), IpAddr::V4(old) if rank(old, &own) <= rank(ip, &own));
            if found.iter().any(|(n, f)| *n == fullname && better(f)) {
                continue;
            }
            found.retain(|(n, _)| *n != fullname);
            found.push((fullname, Found { name, addr: SocketAddr::new(IpAddr::V4(ip), info.get_port()), sharing }));
        }
    }
    let _ = daemon.stop_browse(SERVICE);
    let _ = daemon.shutdown();
    found.into_iter().map(|(_, f)| f).collect()
}

pub fn rank(ip: Ipv4Addr, own: &[Ipv4Addr]) -> u8 {
    let [a, b, c, _] = ip.octets();
    if own.iter().any(|o| o.octets()[..3] == [a, b, c]) {
        0
    } else if a == 192 && b == 168 {
        1
    } else if a == 10 {
        2
    } else if ip.is_private() {
        3
    } else {
        4
    }
}

fn virtual_interface(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["docker", "br-", "veth", "virbr", "vethernet", "virtualbox", "vmware", "vmnet", "tailscale", "zt", "wg", "tun", "tap", "hyper-v"].iter().any(|p| name.contains(p))
}

fn own_addresses() -> Vec<Ipv4Addr> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback() && !virtual_interface(&i.name))
        .filter_map(|i| match i.ip() {
            IpAddr::V4(v4) if v4.is_private() => Some(v4),
            _ => None,
        })
        .collect()
}

fn neighbours() -> Vec<SocketAddr> {
    let own = own_addresses();
    let mut nets = HashSet::new();
    own.iter().filter(|ip| nets.insert([ip.octets()[0], ip.octets()[1], ip.octets()[2]])).flat_map(|ip| {
        let [a, b, c, _] = ip.octets();
        (1..=254u8).map(move |d| Ipv4Addr::new(a, b, c, d))
    })
    .filter(|ip| !own.contains(ip))
    .map(|ip| SocketAddr::new(IpAddr::V4(ip), DEFAULT_PORT))
    .collect()
}

async fn probe() -> Vec<Found> {
    futures_util::stream::iter(neighbours()).map(check).buffer_unordered(PROBES_AT_ONCE).filter_map(|f| async { f }).collect().await
}

pub async fn scan() -> Vec<Found> {
    let mut found = browse().await;
    if found.is_empty() {
        found = probe().await;
    }
    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.addr.cmp(&b.addr)));
    found.dedup_by(|a, b| a.addr == b.addr);
    if let Ok(mut last) = LAST.lock() {
        *last = found.clone();
    }
    found
}

pub fn scan_blocking(limit: Duration) -> Vec<Found> {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
            let _ = send.send(runtime.block_on(scan()));
        }
    });
    receive.recv_timeout(limit).unwrap_or_else(|_| last_found())
}
