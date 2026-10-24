//! OSC over UDP, for Bitfocus Companion, TouchOSC and similar controllers.
//! Messages go to the input layer (`/palto/<action>`). It listens on this
//! computer only unless LAN access is on; other computers must be on the
//! allowed list, since OSC has no pairing of its own.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use tokio::net::UdpSocket;
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OscListen {
    pub enabled: bool,
    /// Listen on all interfaces (LAN access is on).
    pub lan: bool,
    pub port: u16,
    /// Other computers allowed to send; this computer always is.
    pub allowed: Vec<IpAddr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OscArg {
    Int(i32),
    Float(f32),
    Str(String),
    Bool(bool),
}

impl OscArg {
    /// Whether the argument means "released" (0, 0.0 or false), as faders
    /// and buttons on TouchOSC-style surfaces send on release.
    pub fn is_release(&self) -> bool {
        matches!(self, OscArg::Int(0) | OscArg::Bool(false)) || matches!(self, OscArg::Float(f) if *f == 0.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OscStatus {
    pub listening: Option<SocketAddr>,
    pub error: Option<String>,
    /// The last message received: sender, address, when, and whether it was
    /// refused (a sender not on the allowed list).
    pub last: Option<(IpAddr, String, SystemTime, bool)>,
}

/// Receives each allowed message's address and arguments.
pub type OscSink = Arc<dyn Fn(&str, &[OscArg]) + Send + Sync>;

/// A string padded with NULs to a multiple of four bytes.
fn read_string(data: &[u8], at: &mut usize) -> Option<String> {
    let rest = data.get(*at..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    let text = std::str::from_utf8(&rest[..end]).ok()?.to_string();
    *at += (end + 4) & !3;
    Some(text)
}

fn read_u32(data: &[u8], at: &mut usize) -> Option<u32> {
    let bytes = data.get(*at..*at + 4)?;
    *at += 4;
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

/// The messages in one packet (bundles are unpacked; their time tags are
/// ignored, everything acts at once). Malformed parts are skipped.
pub fn parse(packet: &[u8]) -> Vec<(String, Vec<OscArg>)> {
    let mut messages = Vec::new();
    parse_into(packet, &mut messages, 0);
    messages
}

fn parse_into(packet: &[u8], messages: &mut Vec<(String, Vec<OscArg>)>, depth: usize) {
    if depth > 4 {
        return;
    }
    if packet.starts_with(b"#bundle\0") {
        let mut at = 16; // "#bundle\0" and the time tag
        while let Some(size) = read_u32(packet, &mut at) {
            let Some(element) = packet.get(at..at + size as usize) else { return };
            parse_into(element, messages, depth + 1);
            at += size as usize;
        }
        return;
    }
    let mut at = 0;
    let Some(address) = read_string(packet, &mut at).filter(|a| a.starts_with('/')) else { return };
    let tags = read_string(packet, &mut at).unwrap_or_default();
    let mut args = Vec::new();
    for tag in tags.chars().skip_while(|&c| c == ',') {
        let arg = match tag {
            'i' => read_u32(packet, &mut at).map(|v| OscArg::Int(v as i32)),
            'f' => read_u32(packet, &mut at).map(|v| OscArg::Float(f32::from_bits(v))),
            's' => read_string(packet, &mut at).map(OscArg::Str),
            'T' => Some(OscArg::Bool(true)),
            'F' => Some(OscArg::Bool(false)),
            _ => None,
        };
        match arg {
            Some(arg) => args.push(arg),
            None => break,
        }
    }
    messages.push((address, args));
}

/// Listens while enabled, rebinding when the settings change.
pub async fn run(mut config: watch::Receiver<OscListen>, sink: OscSink, status: Arc<Mutex<OscStatus>>, mut shutdown: watch::Receiver<bool>) {
    loop {
        let current = config.borrow_and_update().clone();
        let socket = if current.enabled {
            let ip = if current.lan { IpAddr::V4(Ipv4Addr::UNSPECIFIED) } else { IpAddr::V4(Ipv4Addr::LOCALHOST) };
            let addr = SocketAddr::new(ip, current.port);
            match UdpSocket::bind(addr).await {
                Ok(socket) => {
                    log::info!("OSC listening on udp {addr}");
                    let mut s = status.lock().expect("osc status poisoned");
                    s.listening = Some(addr);
                    s.error = None;
                    Some(socket)
                }
                Err(e) => {
                    log::error!("OSC cannot listen on udp {addr}: {e}");
                    let mut s = status.lock().expect("osc status poisoned");
                    s.listening = None;
                    s.error = Some(format!("Port {} is not available ({e}). Choose another port.", current.port));
                    None
                }
            }
        } else {
            let mut s = status.lock().expect("osc status poisoned");
            s.listening = None;
            s.error = None;
            None
        };
        let mut buffer = vec![0u8; 8192];
        loop {
            tokio::select! {
                _ = shutdown.changed() => return,
                changed = config.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    if *config.borrow() != current {
                        break;
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(5)), if current.enabled && socket.is_none() => break,
                received = async { socket.as_ref().expect("guarded").recv_from(&mut buffer).await }, if socket.is_some() => {
                    let Ok((length, from)) = received else { continue };
                    let allowed = from.ip().is_loopback() || (current.lan && current.allowed.contains(&from.ip()));
                    for (address, args) in parse(&buffer[..length]) {
                        status.lock().expect("osc status poisoned").last = Some((from.ip(), address.clone(), SystemTime::now(), !allowed));
                        if allowed {
                            sink(&address, &args);
                        } else {
                            log::warn!("Ignored OSC {address} from {}, which is not on the allowed list", from.ip());
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(address: &str, tags: &str, payload: &[u8]) -> Vec<u8> {
        let pad = |s: &str| {
            let mut bytes = s.as_bytes().to_vec();
            bytes.push(0);
            while bytes.len() % 4 != 0 {
                bytes.push(0);
            }
            bytes
        };
        [pad(address), pad(tags), payload.to_vec()].concat()
    }

    #[test]
    fn parses_messages_and_bundles() {
        let next = message("/palto/next", ",", &[]);
        assert_eq!(parse(&next), vec![("/palto/next".to_string(), vec![])]);
        let fader = message("/palto/black", ",f", &0.0f32.to_bits().to_be_bytes());
        let parsed = parse(&fader);
        assert_eq!(parsed[0].1, vec![OscArg::Float(0.0)]);
        assert!(parsed[0].1[0].is_release());
        let mut bundle = b"#bundle\0".to_vec();
        bundle.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);
        for element in [&next, &fader] {
            bundle.extend_from_slice(&(element.len() as u32).to_be_bytes());
            bundle.extend_from_slice(element);
        }
        let addresses: Vec<String> = parse(&bundle).into_iter().map(|(a, _)| a).collect();
        assert_eq!(addresses, ["/palto/next", "/palto/black"]);
        assert!(parse(b"garbage").is_empty());
        assert!(parse(&next[..5]).is_empty(), "a truncated packet is skipped");
    }
}
