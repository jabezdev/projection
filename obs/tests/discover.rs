use std::net::Ipv4Addr;

use palto_obs::discover::{parse_address, rank, sharing, Sharing};

#[test]
fn the_address_on_our_own_network_wins() {
    let own = [Ipv4Addr::new(192, 168, 100, 92)];
    let offered = [Ipv4Addr::new(100, 75, 250, 72), Ipv4Addr::new(172, 17, 0, 1), Ipv4Addr::new(192, 168, 100, 40), Ipv4Addr::new(10, 0, 0, 5)];
    assert_eq!(offered.into_iter().min_by_key(|ip| rank(*ip, &own)), Some(Ipv4Addr::new(192, 168, 100, 40)));
    let own = [Ipv4Addr::new(10, 0, 0, 2)];
    assert_eq!(offered.into_iter().min_by_key(|ip| rank(*ip, &own)), Some(Ipv4Addr::new(10, 0, 0, 5)));
}

#[test]
fn feed_versions_decide_who_must_update() {
    assert_eq!(sharing(&[1]), Sharing::On);
    assert_eq!(sharing(&[1, 2]), Sharing::On);
    assert_eq!(sharing(&[2]), Sharing::PluginTooOld);
    assert_eq!(sharing(&[]), Sharing::PaltoTooOld);
}

#[test]
fn typed_addresses_take_the_default_port() {
    assert_eq!(parse_address("192.168.1.20"), "192.168.1.20:8080".parse().ok());
    assert_eq!(parse_address(" http://192.168.1.20:9000/ "), "192.168.1.20:9000".parse().ok());
    assert_eq!(parse_address("Church PC"), None);
}
