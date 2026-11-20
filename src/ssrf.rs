//! Guards the provider URLs users may type in for their own keys. The panel server makes the
//! request, so a user could otherwise point it at the panel's database, Wings, cloud metadata or
//! anything else on the private network.
//!
//! A custom URL passes only when its host is a public name or address and every address the name
//! resolves to is public. The request is then pinned to exactly those addresses (no second DNS
//! lookup, so the name cannot rebind to a private address afterwards) and redirects are refused.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    let private = a == 0 // "this network"
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b)) // carrier-grade NAT, also 100.100.100.200
        || (a == 169 && b == 254) // link-local, cloud metadata
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && (c == 0 || c == 2)) // IETF assignments, documentation
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19)) // benchmarking
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224; // multicast, reserved, broadcast
    !private
}

fn embedded_v4(high: u16, low: u16) -> Ipv4Addr {
    let [a, b] = high.to_be_bytes();
    let [c, d] = low.to_be_bytes();
    Ipv4Addr::new(a, b, c, d)
}

fn public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return public_v4(v4);
    }
    let s = ip.segments();
    // NAT64 (64:ff9b::/96, 64:ff9b:1::/48) and 6to4 (2002::/16) reach the embedded IPv4 address.
    if s[0] == 0x64 && s[1] == 0xff9b {
        return public_v4(embedded_v4(s[6], s[7]));
    }
    if s[0] == 0x2002 {
        return public_v4(embedded_v4(s[1], s[2]));
    }
    let private = ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || s[..6] == [0; 6] // IPv4-compatible ::a.b.c.d
        || (s[0] & 0xfe00) == 0xfc00 // unique local, also fd00:ec2::254
        || (s[0] & 0xffc0) == 0xfe80 // link-local
        || (s[0] & 0xffc0) == 0xfec0 // site-local
        || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
        || (s[0] == 0x0100 && s[1..4] == [0; 3]); // discard
    !private
}

pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => public_v6(ip),
    }
}

/// Names that only mean something inside a network.
fn is_local_name(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    !host.contains('.')
        || host == "localhost"
        || [".localhost", ".local", ".internal", ".lan", ".home.arpa", ".intranet", ".corp"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
}

/// Whether a URL points into the local network. The admin's own provider may (Ollama on the same
/// machine); there it only decides whether an API key is needed.
pub fn is_local_url(url: &str) -> bool {
    match url::Url::parse(url.trim()) {
        Ok(url) => match url.host() {
            Some(url::Host::Ipv4(ip)) => !public_v4(ip),
            Some(url::Host::Ipv6(ip)) => !public_v6(ip),
            Some(url::Host::Domain(name)) => is_local_name(name),
            None => false,
        },
        Err(_) => false,
    }
}

/// Where a vetted request may connect: the host name and the public addresses it resolved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    pub host: String,
    pub addrs: Vec<SocketAddr>,
}

#[async_trait::async_trait]
pub trait Resolver: Send + Sync {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, String>;
}

/// The system resolver.
pub struct Dns;

#[async_trait::async_trait]
impl Resolver for Dns {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
        tokio::net::lookup_host((host, port))
            .await
            .map(|addrs| addrs.collect())
            .map_err(|err| format!("{host} could not be resolved: {err}"))
    }
}

const REFUSED: &str = "points to a local or private network address, which user keys may not use";

/// Checks a user-supplied base URL and returns the addresses the request has to use.
pub async fn vet(url: &str, resolver: &dyn Resolver) -> Result<Pin, String> {
    let parsed = url::Url::parse(url.trim()).map_err(|_| "The base URL is not a valid URL".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("The base URL must start with http:// or https://".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("The base URL must not contain a user name or password".into());
    }
    let port = parsed
        .port_or_known_default()
        .ok_or("The base URL has no port")?;
    let host = match parsed.host() {
        Some(url::Host::Ipv4(ip)) => {
            return public_v4(ip)
                .then(|| Pin {
                    host: ip.to_string(),
                    addrs: vec![SocketAddr::new(IpAddr::V4(ip), port)],
                })
                .ok_or_else(|| format!("The base URL {REFUSED}"));
        }
        Some(url::Host::Ipv6(ip)) => {
            return public_v6(ip)
                .then(|| Pin {
                    host: ip.to_string(),
                    addrs: vec![SocketAddr::new(IpAddr::V6(ip), port)],
                })
                .ok_or_else(|| format!("The base URL {REFUSED}"));
        }
        Some(url::Host::Domain(name)) => name.to_ascii_lowercase(),
        None => return Err("The base URL has no host".into()),
    };
    if is_local_name(&host) {
        return Err(format!("{host} {REFUSED}"));
    }

    let addrs = resolver.resolve(&host, port).await?;
    if addrs.is_empty() {
        return Err(format!("{host} did not resolve to any address"));
    }
    if let Some(bad) = addrs.iter().find(|addr| !is_public(addr.ip())) {
        return Err(format!("{host} resolves to {}, which {REFUSED}", bad.ip()));
    }
    Ok(Pin { host, addrs })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Answers every lookup with fixed addresses and counts the lookups.
    struct Fixed(Vec<IpAddr>, AtomicUsize);

    impl Fixed {
        fn new(ips: &[&str]) -> Self {
            Self(
                ips.iter().map(|ip| ip.parse().unwrap()).collect(),
                AtomicUsize::new(0),
            )
        }
    }

    #[async_trait::async_trait]
    impl Resolver for Fixed {
        async fn resolve(&self, _host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
            self.1.fetch_add(1, Ordering::SeqCst);
            Ok(self.0.iter().map(|ip| SocketAddr::new(*ip, port)).collect())
        }
    }

    #[test]
    fn private_and_special_addresses_are_not_public() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "169.254.170.2",
            "100.100.100.200",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "fd00:ec2::254",
            "fc00::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "::127.0.0.1",
            "64:ff9b::a9fe:a9fe",
            "2002:c0a8:0101::1",
            "ff02::1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip} counted as public");
        }
        for ip in ["1.1.1.1", "104.18.32.47", "2606:4700::6810:84e5", "::ffff:8.8.8.8"] {
            assert!(is_public(ip.parse().unwrap()), "{ip} counted as private");
        }
    }

    #[tokio::test]
    async fn literal_and_local_hosts_are_refused_without_a_lookup() {
        let resolver = Fixed::new(&["1.1.1.1"]);
        for url in [
            "http://127.0.0.1:11434/v1",
            "http://[::1]:8080/v1",
            "http://169.254.169.254/latest",
            "http://[fd00:ec2::254]/",
            "http://localhost:11434/v1",
            "http://ollama.localhost/v1",
            "http://printer.local/v1",
            "http://metadata.google.internal/computeMetadata/v1",
            "http://intranet/v1",
            "file:///etc/passwd",
            "https://user:pass@api.example.com/v1",
        ] {
            assert!(vet(url, &resolver).await.is_err(), "{url} was accepted");
        }
        assert_eq!(resolver.1.load(Ordering::SeqCst), 0);

        let pin = vet("https://1.1.1.1/v1", &resolver).await.unwrap();
        assert_eq!(pin.addrs, vec!["1.1.1.1:443".parse().unwrap()]);
    }

    #[tokio::test]
    async fn a_name_must_resolve_to_public_addresses_only() {
        let public = Fixed::new(&["104.18.32.47", "2606:4700::6810:84e5"]);
        let pin = vet("https://api.example.com/v1", &public).await.unwrap();
        assert_eq!(pin.host, "api.example.com");
        assert_eq!(pin.addrs.len(), 2);
        assert!(pin.addrs.iter().all(|addr| addr.port() == 443));

        // One private answer among public ones is enough to refuse (DNS rebinding setups often
        // return both).
        let mixed = Fixed::new(&["104.18.32.47", "10.0.0.5"]);
        let err = vet("https://rebind.example.com/v1", &mixed).await.unwrap_err();
        assert!(err.contains("10.0.0.5"), "{err}");

        for ip in ["127.0.0.1", "::1", "169.254.169.254", "::ffff:192.168.0.10"] {
            let private = Fixed::new(&[ip]);
            assert!(vet("https://evil.example.com/v1", &private).await.is_err(), "{ip}");
        }
        assert!(vet("https://empty.example.com/v1", &Fixed::new(&[])).await.is_err());
    }

    #[tokio::test]
    async fn the_name_is_resolved_once_and_the_request_is_pinned_to_that_answer() {
        // A rebinding resolver would answer a second lookup with a private address; vet looks up
        // once and the provider request connects to the returned addresses only.
        let resolver = Fixed::new(&["104.18.32.47"]);
        let pin = vet("https://api.example.com:8443/v1", &resolver).await.unwrap();
        assert_eq!(resolver.1.load(Ordering::SeqCst), 1);
        assert_eq!(pin.addrs, vec!["104.18.32.47:8443".parse().unwrap()]);
    }

    #[test]
    fn local_admin_urls_are_recognised() {
        assert!(is_local_url("http://localhost:11434/v1"));
        assert!(is_local_url("http://192.168.1.20:1234/v1"));
        assert!(is_local_url("http://ollama:11434/v1"));
        assert!(!is_local_url("https://api.openai.com/v1"));
        assert!(!is_local_url("https://openrouter.ai/api/v1"));
    }
}
