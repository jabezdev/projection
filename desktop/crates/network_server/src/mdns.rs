use std::net::IpAddr;
use std::sync::Mutex;

use mdns_sd::{ServiceDaemon, ServiceInfo};

use crate::Identity;

pub const HOSTNAME: &str = "palto.local";
const SERVICE_TYPE: &str = "_http._tcp.local.";
const INSTANCE: &str = "PALTO remote";
pub const FEED_SERVICE_TYPE: &str = "_palto._tcp.local.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Advert {
    pub port: u16,
    pub remote: bool,
    pub feed: bool,
}

#[derive(Default)]
pub struct Advertiser {
    identity: Identity,
    running: Mutex<Option<(ServiceDaemon, Vec<String>, Advert)>>,
}

fn virtual_interface(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["docker", "br-", "veth", "virbr", "vethernet", "virtualbox", "vmware", "vmnet", "tailscale", "zt", "wg", "tun", "tap", "hyper-v"]
        .iter()
        .any(|prefix| name.starts_with(prefix) || name.contains(prefix))
}

pub fn lan_addresses() -> Vec<IpAddr> {
    let mut found: Vec<(u8, IpAddr)> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback() && !i.is_link_local() && i.ip().is_ipv4())
        .map(|i| {
            let rank = match i.ip() {
                _ if virtual_interface(&i.name) => 4,
                IpAddr::V4(v4) if v4.octets()[0] == 192 && v4.octets()[1] == 168 => 0,
                IpAddr::V4(v4) if v4.octets()[0] == 10 => 1,
                IpAddr::V4(v4) if v4.is_private() => 2,
                _ => 3,
            };
            (rank, i.ip())
        })
        .collect();
    found.sort();
    let mut addresses: Vec<IpAddr> = found.into_iter().map(|(_, ip)| ip).collect();
    addresses.dedup();
    addresses
}

pub fn instance_name(name: &str) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).map(|c| if c == '.' { '-' } else { c }).take(60).collect();
    match clean.trim() {
        "" => "PALTO".to_string(),
        name => name.to_string(),
    }
}

impl Advertiser {
    pub fn new(identity: Identity) -> Self {
        Self { identity, running: Mutex::default() }
    }

    pub fn set(&self, advert: Option<Advert>) {
        let advert = advert.filter(|a| a.remote || a.feed);
        let mut running = self.running.lock().expect("mdns poisoned");
        if running.as_ref().map(|(_, _, a)| *a) == advert {
            return;
        }
        if let Some((daemon, names, _)) = running.take() {
            for name in names {
                let _ = daemon.unregister(&name);
            }
            let _ = daemon.shutdown();
        }
        let Some(advert) = advert else { return };
        let host = format!("{HOSTNAME}.");
        let addresses = lan_addresses();
        let result = ServiceDaemon::new().and_then(|daemon| {
            let mut names = Vec::new();
            if advert.remote {
                let info = ServiceInfo::new(SERVICE_TYPE, INSTANCE, &host, addresses.as_slice(), advert.port, &[("path", "/")][..])?.enable_addr_auto();
                names.push(info.get_fullname().to_string());
                daemon.register(info)?;
            }
            let feed = if advert.feed { protocol::FEED_VERSION.to_string() } else { "0".to_string() };
            let properties = [("feed", feed.as_str()), ("name", self.identity.name.as_str()), ("version", self.identity.version.as_str())];
            let info = ServiceInfo::new(FEED_SERVICE_TYPE, &instance_name(&self.identity.name), &host, addresses.as_slice(), advert.port, &properties[..])?.enable_addr_auto();
            names.push(info.get_fullname().to_string());
            daemon.register(info)?;
            Ok((daemon, names))
        });
        match result {
            Ok((daemon, names)) => {
                log::info!("Advertising PALTO on port {} (remote {}, OBS feed {})", advert.port, advert.remote, advert.feed);
                *running = Some((daemon, names, advert));
            }
            Err(e) => log::warn!("Cannot advertise {HOSTNAME} on the network: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn virtual_adapters_are_recognized() {
        for name in ["docker0", "br-97b1066d1053", "vEthernet (Default Switch)", "tailscale0", "VirtualBox Host-Only Network"] {
            assert!(super::virtual_interface(name), "{name}");
        }
        for name in ["wlan0", "eth0", "Wi-Fi", "Ethernet", "enp3s0"] {
            assert!(!super::virtual_interface(name), "{name}");
        }
    }
}
