/*!
 * SOURCE OF TRUTH KEYWORDS: network registry, ALLOWED_HOSTS, download_allowlist, HTTP_POLICY, host allowlist, huggingface.co, hf.co CDN, github release assets
 * WHAT:  Every host Echo may ever contact (the model host and its CDN, the release host of the pinned sidecar and
 *        its asset host), the allowlist built from them (https only) and the HTTP client's policy.
 * WHY:   02 §10 names the only outbound traffic: model downloads and the pinned llama.cpp release. The list is one
 *        registry entry per host so a new download source is one line here, reviewed in one place; the HTTP client
 *        refuses everything else, including redirect targets. Hugging Face serves `resolve/` URLs through redirects
 *        to its CDN (`us.aws.cdn.hf.co` today, 05 W42) and GitHub release downloads redirect to
 *        `release-assets.githubusercontent.com` / `objects.githubusercontent.com`, so those parents allow their
 *        subdomains; the entry hosts themselves do not. A test holds every downloadable manifest URL to this list.
 * WHERE: app/bootstrap hands `download_allowlist()` and HTTP_POLICY to adapters/net (HttpClient).
 */

use crate::types::{AllowedHost, HostAllowlist, HttpPolicy, StaticList, StaticStr};

/// Every host the HTTP client may contact.
pub const ALLOWED_HOSTS: &[AllowedHost] = &[
    // Model manifests' `resolve/` URLs (02 §8.2).
    AllowedHost {
        domain: StaticStr::new("huggingface.co"),
        include_subdomains: false,
    },
    // Hugging Face's CDN, where `resolve/` redirects large files.
    AllowedHost {
        domain: StaticStr::new("hf.co"),
        include_subdomains: true,
    },
    // The pinned llama.cpp release (step 23).
    AllowedHost {
        domain: StaticStr::new("github.com"),
        include_subdomains: false,
    },
    // GitHub's release asset host, where release downloads redirect.
    AllowedHost {
        domain: StaticStr::new("githubusercontent.com"),
        include_subdomains: true,
    },
];

/// How the HTTP client connects, waits and follows redirects.
pub const HTTP_POLICY: HttpPolicy = HttpPolicy::DEFAULT;

/// The allowlist the app's HTTP client enforces: ALLOWED_HOSTS over https only.
pub const fn download_allowlist() -> HostAllowlist {
    HostAllowlist {
        hosts: StaticList::new(ALLOWED_HOSTS),
        require_https: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models::MODELS;

    /// The scheme and host of an absolute URL, without a parser dependency.
    fn scheme_and_host(url: &str) -> (&str, &str) {
        let (scheme, rest) = url.split_once("://").unwrap();
        let authority = rest.split(['/', '?', '#']).next().unwrap();
        (
            scheme,
            authority
                .rsplit_once('@')
                .map_or(authority, |(_, host)| host),
        )
    }

    #[test]
    fn every_downloadable_model_file_is_on_the_allowlist() {
        let allowlist = download_allowlist();
        for manifest in MODELS.iter().filter(|manifest| !manifest.bundled) {
            for file in manifest.files.iter() {
                let (scheme, host) = scheme_and_host(file.url.as_str());
                assert!(allowlist.allows(scheme, host), "{}", file.url);
            }
        }
    }

    #[test]
    fn the_allowlist_is_https_only_and_lowercase() {
        let allowlist = download_allowlist();
        assert!(allowlist.require_https);
        for host in ALLOWED_HOSTS {
            assert_eq!(host.domain.as_str(), host.domain.to_ascii_lowercase());
            assert!(!allowlist.allows("http", host.domain.as_str()));
        }
        assert!(allowlist.allows("https", "us.aws.cdn.hf.co"));
        assert!(allowlist.allows("https", "release-assets.githubusercontent.com"));
        assert!(!allowlist.allows("https", "api.github.com"));
        assert!(!allowlist.allows("https", "example.com"));
    }
}
