//! Advertises the remote on the local network as `palto.local`, so phones
//! and browsers reach it by name. Devices that do not resolve `.local`
//! names use the address shown on the operator screen instead.

use std::net::IpAddr;
use std::sync::Mutex;

use mdns_sd::{ServiceDaemon, ServiceInfo};

pub const HOSTNAME: &str = "palto.local";
const SERVICE_TYPE: &str = "_http._tcp.local.";
const INSTANCE: &str = "PALTO remote";

#[derive(Default)]
pub struct Advertiser {
    running: Mutex<Option<(ServiceDaemon, String, u16)>>,
}

/// Interfaces of virtual machines, containers and VPNs: listed last, since
/// phones on the Wi-Fi cannot reach them.
fn virtual_interface(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["docker", "br-", "veth", "virbr", "vethernet", "virtualbox", "vmware", "vmnet", "tailscale", "zt", "wg", "tun", "tap", "hyper-v"]
        .iter()
        .any(|prefix| name.starts_with(prefix) || name.contains(prefix))
}

/// This computer's IPv4 addresses other devices can reach: home and office
/// ranges first (192.168, then 10, then 172.16–31), virtual adapters last.
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

impl Advertiser {
    /// Advertises on `port`, or stops with `None`.
    pub fn set(&self, port: Option<u16>) {
        let mut running = self.running.lock().expect("mdns poisoned");
        if running.as_ref().map(|(_, _, p)| *p) == port {
            return;
        }
        if let Some((daemon, fullname, _)) = running.take() {
            let _ = daemon.unregister(&fullname);
            let _ = daemon.shutdown();
        }
        let Some(port) = port else { return };
        let result = ServiceDaemon::new().and_then(|daemon| {
            let info = ServiceInfo::new(SERVICE_TYPE, INSTANCE, &format!("{HOSTNAME}."), lan_addresses().as_slice(), port, &[("path", "/")][..])?
                .enable_addr_auto();
            let fullname = info.get_fullname().to_string();
            daemon.register(info)?;
            Ok((daemon, fullname))
        });
        match result {
            Ok((daemon, fullname)) => {
                log::info!("Advertising the remote as http://{HOSTNAME}:{port}/");
                *running = Some((daemon, fullname, port));
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
