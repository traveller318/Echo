/*!
 * SOURCE OF TRUTH KEYWORDS: HostAllowlist, AllowedHost, HttpPolicy, allowed hosts, https only, redirect hosts, download timeouts, network policy
 * WHAT:  The network policy shapes: which hosts Echo may contact (HostAllowlist of AllowedHost, https only) and how
 *        the one HTTP client behaves (HttpPolicy: timeouts, redirect limit, system proxy).
 * WHY:   02 §10: only the model host and the pinned sidecar release host are ever contacted, and one client enforces
 *        it, including on every redirect hop (Hugging Face answers from `*.hf.co`, 05 W42). The list is registry data
 *        (registry/network.rs) and the client is an adapter, which may not read the registry (02 §3.2), so the shape
 *        lives here and app/ hands it over. Matching is on whole DNS labels (`cdn.hf.co` matches `hf.co`,
 *        `evilhf.co` does not) and case-insensitive; `require_https` is off only for the tests' loopback server.
 *        A read timeout, not a total one, bounds a stalled transfer: a 670 MB download may take many minutes, but a
 *        connection that delivers nothing for `read_timeout_ms` is dead (Wi-Fi off) and the download resumes later.
 * WHERE: Built by registry/network.rs (`download_allowlist`, HttpPolicy::DEFAULT); taken by adapters/net
 *        (HttpClient) from app/bootstrap.
 */

use super::{StaticList, StaticStr};

/// One host (and optionally its subdomains) the HTTP client may contact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowedHost {
    /// Lowercase DNS name, e.g. `huggingface.co`.
    pub domain: StaticStr,
    /// Also allow every name that ends in `.<domain>` (a CDN behind the host).
    pub include_subdomains: bool,
}

impl AllowedHost {
    /// Whether `host` (a DNS name, any case, optionally with a trailing dot) is this entry.
    pub fn matches(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        let domain = self.domain.as_str();
        host == domain
            || (self.include_subdomains
                && host
                    .strip_suffix(domain)
                    .is_some_and(|prefix| prefix.len() > 1 && prefix.ends_with('.')))
    }
}

/// Every host the HTTP client may contact, and whether plain http is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostAllowlist {
    pub hosts: StaticList<AllowedHost>,
    /// Refuse any URL that is not https (always on in the app).
    pub require_https: bool,
}

impl HostAllowlist {
    /// Whether a URL with this scheme and host may be requested.
    pub fn allows(&self, scheme: &str, host: &str) -> bool {
        let scheme_ok = scheme.eq_ignore_ascii_case("https")
            || (!self.require_https && scheme.eq_ignore_ascii_case("http"));
        scheme_ok && self.hosts.iter().any(|allowed| allowed.matches(host))
    }
}

/// How the HTTP client connects, waits and follows redirects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpPolicy {
    /// Longest wait for a connection (DNS, TCP, TLS).
    pub connect_timeout_ms: u32,
    /// Longest wait for the next bytes of a response; a stalled download fails with `Network` and resumes later.
    pub read_timeout_ms: u32,
    /// Most redirects followed for one request (each hop is checked against the allowlist).
    pub max_redirects: u8,
    /// Route requests through the proxy Windows is configured with.
    pub system_proxy: bool,
}

impl HttpPolicy {
    pub const DEFAULT: Self = Self {
        connect_timeout_ms: 15_000,
        read_timeout_ms: 20_000,
        max_redirects: 10,
        system_proxy: true,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOSTS: &[AllowedHost] = &[
        AllowedHost {
            domain: StaticStr::new("huggingface.co"),
            include_subdomains: false,
        },
        AllowedHost {
            domain: StaticStr::new("hf.co"),
            include_subdomains: true,
        },
    ];

    const LIST: HostAllowlist = HostAllowlist {
        hosts: StaticList::new(HOSTS),
        require_https: true,
    };

    #[test]
    fn hosts_match_whole_labels_only() {
        assert!(LIST.allows("https", "huggingface.co"));
        assert!(LIST.allows("HTTPS", "HuggingFace.co."));
        assert!(LIST.allows("https", "us.aws.cdn.hf.co"));
        assert!(LIST.allows("https", "hf.co"));
        assert!(
            !LIST.allows("https", "cdn.huggingface.co"),
            "no subdomains for this entry"
        );
        assert!(!LIST.allows("https", "evilhf.co"));
        assert!(!LIST.allows("https", ".hf.co"));
        assert!(!LIST.allows("https", "hf.co.evil.com"));
        assert!(!LIST.allows("https", "example.com"));
    }

    #[test]
    fn plain_http_is_refused_unless_allowed() {
        assert!(!LIST.allows("http", "huggingface.co"));
        assert!(!LIST.allows("ftp", "huggingface.co"));
        let loopback = HostAllowlist {
            hosts: StaticList::from(vec![AllowedHost {
                domain: StaticStr::new("127.0.0.1"),
                include_subdomains: false,
            }]),
            require_https: false,
        };
        assert!(loopback.allows("http", "127.0.0.1"));
        assert!(!loopback.allows("ftp", "127.0.0.1"));
    }
}
