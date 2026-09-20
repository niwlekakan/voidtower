//! Provider egress guard: resolve hostnames once and pin the safe result.

use reqwest::{redirect::Policy, Client, Url};
use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

pub async fn client_for(raw_url: &str, timeout: Duration) -> Result<Client, String> {
    client_for_inner(raw_url, timeout, false).await
}

pub async fn client_for_local(raw_url: &str, timeout: Duration) -> Result<Client, String> {
    client_for_inner(raw_url, timeout, true).await
}

async fn client_for_inner(
    raw_url: &str,
    timeout: Duration,
    allow_local: bool,
) -> Result<Client, String> {
    let url = Url::parse(raw_url).map_err(|_| "provider endpoint is invalid".to_string())?;
    let host = url
        .host_str()
        .ok_or_else(|| "provider endpoint has no host".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("provider endpoint must be an HTTP(S) URL without credentials".into());
    }
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
}
