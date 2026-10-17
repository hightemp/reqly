//! Outgoing network policy for the server build (SSRF protection, PRD §5.7).
//!
//! Requests sent by the server must not reach cloud metadata endpoints or,
//! optionally, private networks. Every hop is checked: the host is resolved
//! here, every address must pass, and curl is pinned to a checked address
//! (`--resolve`) so DNS rebinding cannot swap it. Redirects are followed
//! manually so each target goes through the same check.

use std::net::IpAddr;

use reqly_core::{CoreError, CoreResult};
use url::Url;

use crate::engine::{CancelToken, ExecOptions, HttpResponse, ProgressFn, execute};
use crate::prepare::{PreparedBody, PreparedRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    addr: IpAddr,
    prefix: u8,
}

impl Cidr {
    pub fn parse(s: &str) -> CoreResult<Self> {
        let bad = || CoreError::new("policy.invalid_cidr", "invalid CIDR").with_param("value", s);
        let (addr, prefix) = match s.split_once('/') {
            Some((a, p)) => (a, p.parse::<u8>().map_err(|_| bad())?),
            None => (s, if s.contains(':') { 128 } else { 32 }),
        };
        let addr: IpAddr = addr.parse().map_err(|_| bad())?;
        let max = if addr.is_ipv4() { 32 } else { 128 };
        if prefix > max {
            return Err(bad());
        }
        Ok(Self { addr, prefix })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - self.prefix)
                };
                u32::from(net) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - self.prefix)
                };
                u128::from(net) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkPolicy {
    /// Always denied.
    pub deny: Vec<Cidr>,
    /// Hostnames (exact, case-insensitive) that skip all checks.
    pub allow_hosts: Vec<String>,
    /// Deny RFC 1918 / ULA ranges.
    pub block_private: bool,
    /// Deny 127.0.0.0/8 and ::1.
    pub block_loopback: bool,
}

const METADATA_AND_RESERVED: &[&str] = &[
    "0.0.0.0/8",
    "169.254.0.0/16", // link-local, incl. 169.254.169.254 (AWS/GCP/Azure metadata)
    "100.100.100.200/32", // Alibaba Cloud metadata
    "fe80::/10",
    "fd00:ec2::254/128", // AWS IMDS over IPv6
    "::/128",
];

const PRIVATE: &[&str] = &["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "fc00::/7"];
const LOOPBACK: &[&str] = &["127.0.0.0/8", "::1/128"];

fn cidrs(list: &[&str]) -> Vec<Cidr> {
    list.iter()
        .map(|c| Cidr::parse(c).expect("valid built-in CIDR"))
        .collect()
}

impl NetworkPolicy {
    /// Deny metadata and reserved ranges; private networks and loopback are
    /// allowed because self-hosted users usually call internal services.
    pub fn server_default() -> Self {
        Self {
            deny: cidrs(METADATA_AND_RESERVED),
            allow_hosts: Vec::new(),
            block_private: false,
            block_loopback: false,
        }
    }

    pub fn check_ip(&self, ip: IpAddr) -> CoreResult<()> {
        // ::ffff:a.b.c.d is IPv4 in disguise.
        let ip = match ip {
            IpAddr::V6(v6) => v6
                .to_ipv4_mapped()
                .map(IpAddr::V4)
                .unwrap_or(IpAddr::V6(v6)),
            v4 => v4,
        };
        let denied = self.deny.iter().any(|c| c.contains(ip))
            || (self.block_private && cidrs(PRIVATE).iter().any(|c| c.contains(ip)))
            || (self.block_loopback && cidrs(LOOPBACK).iter().any(|c| c.contains(ip)));
        if denied {
            return Err(CoreError::new(
                "http.blocked",
                "destination is blocked by the server network policy",
            )
            .with_param("ip", ip.to_string()));
        }
        Ok(())
    }

    fn host_allowed(&self, host: &str) -> bool {
        self.allow_hosts
            .iter()
            .any(|h| h.eq_ignore_ascii_case(host))
    }

    /// Resolves and checks `url`; returns a curl `--resolve` pin, if any.
    async fn check_url(&self, url: &str) -> CoreResult<Option<String>> {
        let parsed = Url::parse(url).map_err(|e| {
            CoreError::new("http.invalid_url", e.to_string()).with_param("url", url)
        })?;
        let host = parsed
            .host_str()
            .ok_or_else(|| {
                CoreError::new("http.invalid_url", "URL has no host").with_param("url", url)
            })?
            .trim_matches(|c| c == '[' || c == ']')
            .to_string();
        if self.host_allowed(&host) {
            return Ok(None);
        }
        let port = parsed.port_or_known_default().unwrap_or(80);
        if let Ok(ip) = host.parse::<IpAddr>() {
            self.check_ip(ip)?;
            return Ok(None);
        }
        let addrs: Vec<IpAddr> = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|e| {
                CoreError::new("http.dns", e.to_string()).with_param("host", host.clone())
            })?
            .map(|a| a.ip())
            .collect();
        if addrs.is_empty() {
            return Err(
                CoreError::new("http.dns", "host has no addresses").with_param("host", host)
            );
        }
        for ip in &addrs {
            self.check_ip(*ip)?;
        }
        let pinned = match addrs[0] {
            IpAddr::V6(v6) => format!("[{v6}]"),
            v4 => v4.to_string(),
        };
        Ok(Some(format!("{host}:{port}:{pinned}")))
    }
}

fn not_allowed(setting: &str) -> CoreError {
    CoreError::new(
        "http.not_allowed",
        "this setting is not available on the server",
    )
    .with_param("setting", setting)
}

fn parse_ip(text: &str) -> Option<IpAddr> {
    text.trim()
        .trim_matches(|c| c == '[' || c == ']')
        .parse()
        .ok()
}

impl NetworkPolicy {
    /// Checks the user's network overrides; returns pins for `connect-to` hosts.
    async fn check_overrides(
        &self,
        settings: &reqly_core::model::RequestSettings,
    ) -> CoreResult<Vec<String>> {
        if settings
            .unix_socket
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
        {
            return Err(not_allowed("unixSocket"));
        }
        if settings
            .interface
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
        {
            return Err(not_allowed("interface"));
        }
        for line in settings
            .resolve
            .iter()
            .flatten()
            .filter(|l| !l.trim().is_empty())
        {
            let addresses = line.trim().splitn(3, ':').nth(2).unwrap_or("");
            for addr in addresses.split(',') {
                let ip = parse_ip(addr).ok_or_else(|| {
                    CoreError::new("http.invalid_setting", format!("not an IP address: {addr}"))
                        .with_param("setting", "resolve")
                })?;
                self.check_ip(ip)?;
            }
        }
        let mut pins = Vec::new();
        for line in settings
            .connect_to
            .iter()
            .flatten()
            .filter(|l| !l.trim().is_empty())
        {
            let parts: Vec<&str> = line.trim().split(':').collect();
            let (Some(host), Some(port)) = (parts.get(2), parts.get(3)) else {
                continue;
            };
            if host.is_empty() {
                continue;
            }
            let url = format!(
                "http://{host}:{}/",
                if port.is_empty() { "80" } else { port }
            );
            if let Some(pin) = self.check_url(&url).await? {
                pins.push(pin);
            }
        }
        Ok(pins)
    }
}

/// Executes `request` honoring `policy`, following redirects hop by hop.
pub async fn send_checked(
    mut request: PreparedRequest,
    options: ExecOptions,
    cancel: CancelToken,
    mut progress: Option<ProgressFn>,
    policy: &NetworkPolicy,
) -> CoreResult<HttpResponse> {
    let follow = request.settings.follow_redirects.unwrap_or(true);
    let max_redirects = request.settings.max_redirects.unwrap_or(10);
    request.settings.follow_redirects = Some(false);

    let mut options = options;
    let override_pins = policy.check_overrides(&request.settings).await?;
    // Hosts the user resolves explicitly (already checked) are not re-pinned.
    let user_resolved: Vec<String> = request
        .settings
        .resolve
        .iter()
        .flatten()
        .filter_map(|l| {
            let mut parts = l.trim().splitn(3, ':');
            Some(format!("{}:{}:", parts.next()?, parts.next()?).to_ascii_lowercase())
        })
        .collect();
    let mut timeline = Vec::new();
    let mut total_ms = 0.0;
    let mut redirects = 0u32;

    loop {
        let target = Url::parse(&request.url).ok().and_then(|u| {
            Some(format!("{}:{}:", u.host_str()?, u.port_or_known_default()?).to_ascii_lowercase())
        });
        let pin = match target {
            // Resolved by the user to addresses that were already checked.
            Some(t) if user_resolved.contains(&t) => None,
            _ => policy.check_url(&request.url).await?,
        };
        options.resolve = pin
            .into_iter()
            .chain(override_pins.iter().cloned())
            .collect();

        let mut res = execute(
            request.clone(),
            options.clone(),
            cancel.clone(),
            progress.take(),
        )
        .await?;
        total_ms += res.timings.total_ms;
        timeline.append(&mut res.timeline);

        let location = res
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("location"))
            .map(|(_, v)| v.clone());
        let is_redirect = matches!(res.status, 301 | 302 | 303 | 307 | 308);
        let (true, true, Some(location)) = (follow, is_redirect, location) else {
            res.timeline = timeline;
            res.timings.total_ms = total_ms;
            res.redirects = redirects;
            return Ok(res);
        };

        if redirects >= max_redirects {
            return Err(
                CoreError::new("http.too_many_redirects", "maximum redirects followed")
                    .with_param("max", max_redirects),
            );
        }
        redirects += 1;

        let next = Url::parse(&request.url)
            .and_then(|base| base.join(&location))
            .map_err(|e| {
                CoreError::new("http.invalid_url", e.to_string())
                    .with_param("url", location.clone())
            })?;
        // Browser semantics: 303 always becomes GET; 301/302 turn POST into GET.
        let to_get =
            res.status == 303 || (matches!(res.status, 301 | 302) && request.method == "POST");
        if to_get {
            if request.method != "HEAD" {
                request.method = "GET".into();
            }
            request.body = PreparedBody::None;
            request.headers.retain(|(k, _)| {
                !k.eq_ignore_ascii_case("content-type") && !k.eq_ignore_ascii_case("content-length")
            });
        }
        let same_origin = Url::parse(&request.url).map(|u| u.origin()) == Ok(next.origin());
        if !same_origin {
            // Never leak credentials to another origin.
            request.headers.retain(|(k, _)| {
                !k.eq_ignore_ascii_case("authorization") && !k.eq_ignore_ascii_case("cookie")
            });
        }
        request.url = next.to_string();
        if !res.cookies.is_empty() {
            options.cookies = std::mem::take(&mut res.cookies);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::*;

    #[test]
    fn cidr_matching() {
        let c = Cidr::parse("10.0.0.0/8").unwrap();
        assert!(c.contains(IpAddr::V4(Ipv4Addr::new(10, 255, 0, 1))));
        assert!(!c.contains(IpAddr::V4(Ipv4Addr::new(11, 0, 0, 1))));
        let all = Cidr::parse("0.0.0.0/0").unwrap();
        assert!(all.contains(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))));
        let v6 = Cidr::parse("fe80::/10").unwrap();
        assert!(v6.contains(IpAddr::V6("fe80::abcd".parse::<Ipv6Addr>().unwrap())));
        assert!(Cidr::parse("1.2.3.4/33").is_err());
        assert!(Cidr::parse("nope").is_err());
    }
}
