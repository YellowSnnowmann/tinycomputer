//! The pages a session may show: `SessionOptions::allowed_origins`, read once
//! when the session opens and asked of every page it navigates to.
//!
//! Only pages are checked: a navigation before it is sent, and the page every
//! call leaves the session on. The files a page loads are never checked. A
//! site draws its pages from its own CDN and calls APIs on other hosts, and
//! refusing those breaks the page (pictures and scripts missing, suggestion
//! lists empty) without keeping the agent anywhere it could not already go.
//! The list is a guard rail, not a sandbox.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// What a session's `allowed_origins` admits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Origins {
    /// Whether a list was given: a list admits only what one of its entries
    /// names, even when no entry could be read.
    restricted: bool,
    entries: Vec<Entry>,
}

/// One entry of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    /// `*`: any public host. Private, loopback, and link-local addresses, and
    /// `localhost` and `.local` names, stay refused.
    Public,
    /// `https://example.com` or `example.com`: that host alone.
    Host(String),
    /// `.example.com`, `https://.example.com`, or `*.example.com`: the host
    /// and every subdomain of it.
    Domain(String),
}

impl Origins {
    /// The origins `list` names, spelled as `SessionOptions::allowed_origins`
    /// spells them. An entry that names no host is skipped.
    pub(crate) fn new(list: &[String]) -> Self {
        Self {
            restricted: !list.is_empty(),
            entries: list
                .iter()
                .filter_map(|origin| Entry::parse(origin))
                .collect(),
        }
    }

    /// Whether a list was given, so pages are checked at all.
    pub(crate) fn restricts(&self) -> bool {
        self.restricted
    }

    /// Whether a page at `url` may show. Any page may when no list was given.
    /// Under a list, a page that is no site (`about:blank`, a browser error
    /// page) may, a web page may when an entry names its host, and any other
    /// scheme (a file, `data:`, a browser setting) may not. An address with no
    /// scheme is read as `https://`, as the browser reads it.
    pub(crate) fn admits(&self, url: &str) -> bool {
        if !self.restricted {
            return true;
        }
        let url = url.trim();
        let (scheme, rest) = match url.split_once("://") {
            Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
            None => match url.split_once(':') {
                Some((scheme, rest))
                    if NON_NETWORK_SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()) =>
                {
                    (scheme.to_ascii_lowercase(), rest)
                }
                _ => ("https".to_owned(), url),
            },
        };
        match scheme.as_str() {
            "about" | "chrome-error" => true,
            "http" | "https" | "ws" | "wss" => host_name(authority(rest))
                .is_some_and(|host| self.entries.iter().any(|entry| entry.admits(&host))),
            _ => false,
        }
    }
}

/// Schemes written without `//` that name no host: an address starting with
/// one of them is read as that scheme, never as a bare host.
const NON_NETWORK_SCHEMES: &[&str] = &[
    "about",
    "blob",
    "chrome",
    "chrome-error",
    "data",
    "file",
    "javascript",
    "view-source",
];

impl Entry {
    /// The entry `origin` spells, or `None` when it names no host.
    fn parse(origin: &str) -> Option<Self> {
        let origin = origin.trim();
        if origin == "*" {
            return Some(Self::Public);
        }
        let rest = origin.split_once("://").map_or(origin, |(_, rest)| rest);
        let authority = authority(rest);
        match authority
            .strip_prefix("*.")
            .or_else(|| authority.strip_prefix('.'))
        {
            Some(domain) => host_name(domain).map(Self::Domain),
            None => host_name(authority).map(Self::Host),
        }
    }

    /// Whether this entry names `host`, a host as [`host_name`] reads it.
    fn admits(&self, host: &str) -> bool {
        match self {
            Self::Public => !private_or_local(host),
            Self::Host(named) => host == named,
            Self::Domain(domain) => {
                host == domain
                    || host
                        .strip_suffix(domain.as_str())
                        .is_some_and(|subdomain| subdomain.ends_with('.'))
            }
        }
    }
}

/// The authority of an address after its scheme: up to the path, query, or
/// fragment.
fn authority(rest: &str) -> &str {
    let rest = rest.trim_start_matches('/');
    rest.split(['/', '?', '#']).next().unwrap_or_default()
}

/// The host of `authority`, in lower case, without credentials, port, the
/// brackets of an IPv6 address, or a trailing dot. `None` when there is none.
fn host_name(authority: &str) -> Option<String> {
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = match authority.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Whether `host` is a name or address that only reaches this machine or its
/// network: `localhost`, a `.localhost` or `.local` name, or a private,
/// loopback, link-local, shared, documentation, or other non-global address.
fn private_or_local(host: &str) -> bool {
    // The last label decides: `localhost`, `app.localhost`, `printer.local`.
    if matches!(host.rsplit('.').next(), Some("localhost" | "local")) {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(address)) => non_global_v4(address),
        Ok(IpAddr::V6(address)) => non_global_v6(address),
        Err(_) => false,
    }
}

/// Whether an IPv4 address is outside the public internet.
fn non_global_v4(address: Ipv4Addr) -> bool {
    let [first, second, third, _] = address.octets();
    address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_unspecified()
        || address.is_broadcast()
        || address.is_documentation()
        || address.is_multicast()
        // This network (0.0.0.0/8), shared address space (100.64.0.0/10),
        // protocol assignments (192.0.0.0/24), benchmarking (198.18.0.0/15),
        // and reserved (240.0.0.0/4).
        || first == 0
        || (first == 100 && (64..=127).contains(&second))
        || (first == 192 && second == 0 && third == 0)
        || (first == 198 && (18..=19).contains(&second))
        || first >= 240
}

/// Whether an IPv6 address is outside the public internet; an IPv4 address
/// carried in one is judged as itself.
fn non_global_v6(address: Ipv6Addr) -> bool {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return non_global_v4(mapped);
    }
    let [first, second, ..] = address.segments();
    address.is_loopback()
        || address.is_unspecified()
        || address.is_multicast()
        // Unique local (fc00::/7), link-local (fe80::/10), and
        // documentation (2001:db8::/32).
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || (first == 0x2001 && second == 0x0db8)
}

#[cfg(test)]
mod origins_tests;
