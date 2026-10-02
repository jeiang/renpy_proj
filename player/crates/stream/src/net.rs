//! Interface enumeration, candidate addresses and URLs.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// IPv4 addresses of every configured interface (getifaddrs lists only assigned addresses): `(address, is_loopback)`.
pub(crate) fn interface_v4() -> Vec<(Ipv4Addr, bool)> {
    let mut v: Vec<_> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| match i.ip() {
            IpAddr::V4(a) => Some((a, i.is_loopback())),
            IpAddr::V6(_) => None,
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Host candidate socket addresses for a media socket bound to `bind:port`.
/// An unspecified bind advertises every non-loopback IPv4 interface and loopback;
/// a specific bind advertises that address only.
pub(crate) fn candidate_addrs(
    bind: IpAddr,
    port: u16,
    ifaces: &[(Ipv4Addr, bool)],
) -> Vec<SocketAddr> {
    match bind {
        IpAddr::V4(a) if a.is_unspecified() => {
            let mut out: Vec<_> = ifaces
                .iter()
                .filter(|(_, lo)| !lo)
                .map(|(a, _)| *a)
                .collect();
            out.extend(ifaces.iter().filter(|(_, lo)| *lo).map(|(a, _)| *a));
            out.into_iter()
                .map(|a| SocketAddr::new(a.into(), port))
                .collect()
        }
        IpAddr::V6(a) if a.is_unspecified() => {
            candidate_addrs(Ipv4Addr::UNSPECIFIED.into(), port, ifaces)
        }
        ip => vec![SocketAddr::new(ip, port)],
    }
}

/// `http://<ip>:<port>/` for every non-loopback IPv4 interface, plus loopback when bound to it.
pub(crate) fn urls(bind: IpAddr, port: u16, ifaces: &[(Ipv4Addr, bool)]) -> Vec<String> {
    let addrs: Vec<IpAddr> = match bind {
        IpAddr::V4(a) if a.is_unspecified() => ifaces
            .iter()
            .filter(|(_, lo)| !lo)
            .map(|(a, _)| IpAddr::V4(*a))
            .collect(),
        IpAddr::V6(a) if a.is_unspecified() => ifaces
            .iter()
            .filter(|(_, lo)| !lo)
            .map(|(a, _)| IpAddr::V4(*a))
            .collect(),
        ip => vec![ip],
    };
    addrs
        .into_iter()
        .map(|a| format!("http://{a}:{port}/"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ifs() -> Vec<(Ipv4Addr, bool)> {
        vec![
            (Ipv4Addr::new(198, 51, 100, 7), false),
            (Ipv4Addr::new(127, 0, 0, 1), true),
            (Ipv4Addr::new(192, 0, 2, 20), false),
        ]
    }

    #[test]
    fn wildcard_bind_advertises_every_interface() {
        let c = candidate_addrs(Ipv4Addr::UNSPECIFIED.into(), 5000, &ifs());
        let s: Vec<String> = c.iter().map(|a| a.to_string()).collect();
        assert_eq!(
            s,
            ["198.51.100.7:5000", "192.0.2.20:5000", "127.0.0.1:5000"]
        );
    }

    #[test]
    fn specific_bind_advertises_only_itself() {
        let c = candidate_addrs("127.0.0.1".parse().unwrap(), 7, &ifs());
        assert_eq!(c, ["127.0.0.1:7".parse::<SocketAddr>().unwrap()]);
    }

    #[test]
    fn urls_skip_loopback_on_wildcard() {
        assert_eq!(
            urls(Ipv4Addr::UNSPECIFIED.into(), 80, &ifs()),
            ["http://198.51.100.7:80/", "http://192.0.2.20:80/"]
        );
        assert_eq!(
            urls("127.0.0.1".parse().unwrap(), 80, &ifs()),
            ["http://127.0.0.1:80/"]
        );
    }
}
