//! Who is really on the other end of a connection. Behind a reverse proxy every connection comes from the proxy, so the
//! address that counts is the one the proxy says it got the request from (`X-Forwarded-For`). That is believed only when
//! the connection is from an address the master was told to trust (`FAFNIRX_TRUSTED_PROXY`); from anyone else the header
//! is just text that anyone could have written.

use anyhow::{bail, Context, Result};
use std::net::IpAddr;

/// An address or a range of them (`172.18.0.2`, `10.0.0.0/8`, `fd00::/8`).
#[derive(Clone, Debug, PartialEq)]
struct Range {
    network: IpAddr,
    bits: u8,
}

impl Range {
    fn parse(text: &str) -> Result<Self> {
        let (address, bits) = match text.split_once('/') {
            Some((address, bits)) => (address, Some(bits)),
            None => (text, None),
        };
        let network: IpAddr = address.trim().parse().with_context(|| format!("{text:?} is not an address"))?;
        let widest = if network.is_ipv4() { 32 } else { 128 };
        let bits = match bits {
            Some(bits) => bits
                .trim()
                .parse::<u8>()
                .ok()
                .filter(|b| *b <= widest)
                .with_context(|| format!("{text:?}: the prefix must be 0 to {widest}"))?,
            None => widest,
        };
        Ok(Self { network: network.to_canonical(), bits })
    }

    fn contains(&self, ip: IpAddr) -> bool {
        let keep = |width: u32, bits: u8| if bits == 0 { 0 } else { u128::MAX << (width - u32::from(bits)) };
        match (self.network, ip.to_canonical()) {
            (IpAddr::V4(n), IpAddr::V4(i)) => {
                (u128::from(u32::from(n)) ^ u128::from(u32::from(i))) & keep(32, self.bits) & 0xffff_ffff == 0
            }
            (IpAddr::V6(n), IpAddr::V6(i)) => (u128::from(n) ^ u128::from(i)) & keep(128, self.bits) == 0,
            _ => false,
        }
    }
}

/// The proxies whose word about the client's address is taken. None by default: then the connection's own address is used.
#[derive(Clone, Debug, Default)]
pub struct Trusted(Vec<Range>);

impl Trusted {
    /// From `FAFNIRX_TRUSTED_PROXY`: addresses and ranges, separated by commas.
    pub fn from_env() -> Result<Self> {
        match std::env::var("FAFNIRX_TRUSTED_PROXY") {
            Ok(text) => Self::parse(&text).context("FAFNIRX_TRUSTED_PROXY"),
            Err(_) => Ok(Self::default()),
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut ranges = Vec::new();
        for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            if part == "*" {
                bail!("'*' would trust everyone: name the proxy's address or range");
            }
            ranges.push(Range::parse(part)?);
        }
        Ok(Self(ranges))
    }

    fn trusts(&self, ip: IpAddr) -> bool {
        self.0.iter().any(|r| r.contains(ip))
    }

    /// The client's address. `peer` is the connection's, `forwarded` the value of `X-Forwarded-For`, whose entries each
    /// proxy on the way has added to: the last one that is not a trusted proxy itself is the client (the ones before it
    /// are whatever the client claimed).
    pub fn client_ip(&self, peer: IpAddr, forwarded: Option<&str>) -> IpAddr {
        let peer = peer.to_canonical();
        if !self.trusts(peer) {
            return peer;
        }
        let chain: Vec<IpAddr> = forwarded
            .unwrap_or("")
            .split(',')
            .filter_map(|p| p.trim().parse::<IpAddr>().ok())
            .map(|ip| ip.to_canonical())
            .collect();
        chain.iter().rev().find(|ip| !self.trusts(**ip)).or(chain.first()).copied().unwrap_or(peer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn ranges_match_by_their_prefix() {
        let t = Trusted::parse("172.18.0.0/16, 10.1.2.3, fd00::/8").unwrap();
        for yes in ["172.18.0.2", "172.18.255.255", "10.1.2.3", "fd12::1", "::ffff:172.18.0.9"] {
            assert!(t.trusts(ip(yes)), "{yes}");
        }
        for no in ["172.19.0.1", "10.1.2.4", "192.168.1.1", "fe80::1", "::1"] {
            assert!(!t.trusts(ip(no)), "{no}");
        }
        assert!(Trusted::parse("0.0.0.0/0").unwrap().trusts(ip("8.8.8.8")));
        assert!(!Trusted::parse("").unwrap().trusts(ip("127.0.0.1")));
    }

    #[test]
    fn bad_settings_are_refused() {
        for bad in ["nonsense", "10.0.0.0/33", "fd00::/129", "10.0.0.0/x", "*"] {
            assert!(Trusted::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_client_is_believed_only_through_a_trusted_proxy() {
        let t = Trusted::parse("172.18.0.2").unwrap();
        // From the proxy: the address it saw.
        assert_eq!(t.client_ip(ip("172.18.0.2"), Some("203.0.113.9")), ip("203.0.113.9"));
        // The client wrote some of the header itself: only what the proxy added at the end counts.
        assert_eq!(t.client_ip(ip("172.18.0.2"), Some("1.2.3.4, 203.0.113.9")), ip("203.0.113.9"));
        // Proxies in a chain are skipped.
        assert_eq!(t.client_ip(ip("172.18.0.2"), Some("203.0.113.9, 172.18.0.2")), ip("203.0.113.9"));
        // Not from the proxy: the header is not believed.
        assert_eq!(t.client_ip(ip("198.51.100.7"), Some("203.0.113.9")), ip("198.51.100.7"));
        // From the proxy but with nothing usable: the proxy itself.
        assert_eq!(t.client_ip(ip("172.18.0.2"), None), ip("172.18.0.2"));
        assert_eq!(t.client_ip(ip("172.18.0.2"), Some("garbage")), ip("172.18.0.2"));
        // With nobody trusted, the connection is always the answer.
        assert_eq!(Trusted::default().client_ip(ip("172.18.0.2"), Some("203.0.113.9")), ip("172.18.0.2"));
    }
}
