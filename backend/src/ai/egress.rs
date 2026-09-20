//! Provider egress guard: resolve hostnames once and pin the safe result.

use reqwest::{redirect::Policy, Client, Url};
use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

pub const MAX_CONFIGURED_ENDPOINT_LENGTH: usize = 2048;

pub async fn client_for(raw_url: &str, timeout: Duration) -> Result<Client, String> {
    client_for_inner(raw_url, timeout, false).await
}

pub async fn client_for_local(raw_url: &str, timeout: Duration) -> Result<Client, String> {
    client_for_inner(raw_url, timeout, true).await
}

/// Validate a user-configured HTTP endpoint before it is persisted or used.
///
/// Odysseus may run on a private LAN or loopback address, but it must still
/// resolve to a safe address and have no credentials, query, or fragment that
/// could change the meaning of the API base path.
pub async fn validate_local_endpoint(raw_url: &str) -> Result<(), String> {
    client_for_local(raw_url, Duration::from_secs(5))
        .await
        .map(|_| ())
}

fn parse_endpoint(raw_url: &str) -> Result<Url, String> {
    if raw_url.is_empty() {
        return Err("provider endpoint must not be empty".into());
    }
    if raw_url.len() > MAX_CONFIGURED_ENDPOINT_LENGTH {
        return Err("provider endpoint is too long".into());
    }
    let url = Url::parse(raw_url).map_err(|_| "provider endpoint is invalid".to_string())?;
    if url.host_str().is_none() {
        return Err("provider endpoint has no host".into());
    }
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("provider endpoint must be an HTTP(S) URL without credentials".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("provider endpoint must not contain a query or fragment".into());
    }
    if url.port_or_known_default().is_none() {
        return Err("provider endpoint has no port".into());
    }
    Ok(url)
}

async fn client_for_inner(
    raw_url: &str,
    timeout: Duration,
    allow_local: bool,
) -> Result<Client, String> {
    let url = parse_endpoint(raw_url)?;
    let host = url.host_str().expect("parse_endpoint guarantees a host");
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "provider endpoint has no port".to_string())?;
    let addresses = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| "provider endpoint DNS resolution failed".to_string())?
            .collect()
    };
    if addresses.is_empty()
        || addresses.iter().any(|address| {
            is_prohibited(address.ip()) && !(allow_local && is_private_local(address.ip()))
        })
    {
        return Err("provider endpoint resolves to a prohibited network address".into());
    }
    let mut builder = Client::builder()
        .timeout(timeout)
        .redirect(Policy::none())
        .no_proxy();
    for address in addresses {
        builder = builder.resolve(host, address);
    }
    builder
        .build()
        .map_err(|_| "provider HTTP client unavailable".into())
}

fn is_private_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private(),
        IpAddr::V6(ip) => ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00,
    }
}

pub fn is_prohibited(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let o = ip.octets();
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 192 && o[1] == 0 && o[2] == 2)
                || (o[0] == 198 && (18..=19).contains(&o[1]))
                || (o[0] == 198 && o[1] == 51 && o[2] == 100)
                || (o[0] == 203 && o[1] == 0 && o[2] == 113)
                || o[0] >= 240
        }
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] & 0xffc0) == 0xfec0
                || (segments[0] == 0x2001
                    && ((segments[1] == 0x0000)
                        || (segments[1] == 0x0001)
                        || (segments[1] == 0x0002 && segments[2] == 0)
                        || (segments[1] == 0x0003)
                        || (segments[1] == 0x0004)
                        || (segments[1] == 0x0005)
                        || (segments[1] & 0xfff0) == 0x0010
                        || (segments[1] & 0xfff0) == 0x0020
                        || segments[1] == 0x0db8))
                || segments[0] == 0x2002
                || (segments[0] == 0x3fff && (segments[1] & 0xf000) == 0)
                || segments[0] == 0x5f00
                || ip
                    .to_ipv4_mapped()
                    .is_some_and(|mapped| is_prohibited(IpAddr::V4(mapped)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_prohibited;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    #[test]
    fn rejects_private_metadata_and_special_ranges() {
        for value in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "198.18.0.1",
            "203.0.113.9",
            "fec0::1",
            "2001:db8::1",
            "2001:2::1",
            "2001:10::1",
            "3fff::1",
        ] {
            assert!(is_prohibited(value.parse().unwrap()), "{value}");
        }
        assert!(is_prohibited(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(is_prohibited("::ffff:127.0.0.1".parse().unwrap()));
        assert!(is_prohibited(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
    }
    #[test]
    fn permits_public_addresses() {
        assert!(!is_prohibited("1.1.1.1".parse().unwrap()));
        assert!(!is_prohibited("2606:4700:4700::1111".parse().unwrap()));
    }

    #[tokio::test]
    async fn resolves_and_rejects_localhost_before_request() {
        let error = super::client_for("http://localhost:11434", std::time::Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(error.contains("prohibited network address"));
    }

    #[tokio::test]
    async fn local_provider_may_pin_loopback_but_not_metadata() {
        assert!(super::client_for_local(
            "http://localhost:11434",
            std::time::Duration::from_secs(1)
        )
        .await
        .is_ok());
        assert!(super::client_for_local(
            "http://169.254.169.254",
            std::time::Duration::from_secs(1)
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn configured_endpoint_rejects_credentials_queries_and_oversized_values() {
        for value in [
            "http://user:password@127.0.0.1:11434",
            "http://127.0.0.1:11434/?redirect=https://example.com",
        ] {
            let error = super::validate_local_endpoint(value).await.unwrap_err();
            assert!(
                error.contains("credentials") || error.contains("query"),
                "unexpected error for {value}: {error}"
            );
        }

        let oversized = format!("http://127.0.0.1:11434/{}", "x".repeat(2048));
        let error = super::validate_local_endpoint(&oversized)
            .await
            .unwrap_err();
        assert!(error.contains("too long"));
        assert!(
            super::client_for_local(&oversized, std::time::Duration::from_secs(1))
                .await
                .is_err()
        );
        assert!(super::client_for_local(
            "http://127.0.0.1:11434/?unsafe=query",
            std::time::Duration::from_secs(1)
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn configured_endpoint_rejects_metadata_and_accepts_local_base() {
        let error = super::validate_local_endpoint("http://169.254.169.254")
            .await
            .unwrap_err();
        assert!(error.contains("prohibited network address"));

        super::validate_local_endpoint("http://127.0.0.1:11434/base")
            .await
            .unwrap();
    }
}
