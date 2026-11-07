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
