/*!
 * SOURCE OF TRUTH KEYWORDS: HttpClient, allowlisted HTTP, host allowlist, redirect check, Range request, resume download, rustls ring provider, network gate, offline mode
 * WHAT:  HttpClient: the only HTTP client in Echo. `get_from(url, offset)` sends a GET (a `Range: bytes=offset-`
 *        request when resuming) to an allowlisted https host and returns the body as a stream of chunks
 *        (HttpBody), or says the server cannot serve that range (HttpFetch::RangeNotSatisfiable).
 * WHY:   02 §10: outbound traffic is refused unless its host is on the registry allowlist, and that includes every
 *        redirect hop (a CDN answering for Hugging Face must itself be allowed; anything else is an error, not a
 *        silent follow). The network PermissionGate is asked before each request and before each chunk, so
 *        switching offline mode on stops a running download within one chunk with the factory's own error
 *        (its denial error, `Offline`). TLS is rustls on the `ring` provider (no cmake or NASM, 05 W24),
 *        installed as the process default before the client is built because reqwest panics without one
 *        (05 W41); certificates are checked by Windows through rustls-platform-verifier. A read timeout, not a total
 *        one, ends a stalled transfer (HttpPolicy), and every failure is `Network` with the cause chain as log
 *        detail (never a URL query: CDN URLs carry signatures). No cookies, no referer, a fixed user agent.
 * WHERE: Built by app/bootstrap with registry/network.rs's allowlist and policy and the network gate; owned by
 *        HttpModelStore (adapters/net/model_store), which also downloads the llama.cpp runtime archive; the
 *        loopback client (loopback.rs) shares its TLS provider install.
 */

use std::{error::Error as StdError, time::Duration};

use reqwest::{
    Client, Response, StatusCode, Url,
    header::{CONTENT_RANGE, RANGE},
    redirect,
};

use crate::types::{AppError, HostAllowlist, HttpPolicy, PermissionGate, PortError, PortResult};

/// The one HTTP client: allowlisted hosts only, gated by the network permission.
pub struct HttpClient {
    client: Client,
    allowlist: HostAllowlist,
    gate: PermissionGate,
}

/// What a GET returned.
pub enum HttpFetch {
    /// The body, starting at `HttpBody::start`.
    Body(HttpBody),
    /// The server has no bytes from the requested offset (416): the local copy is longer than the file or stale.
    RangeNotSatisfiable,
}

/// A response body being received.
pub struct HttpBody {
    response: Response,
    start: u64,
    gate: PermissionGate,
}

impl HttpClient {
    /// A client that contacts only `allowlist`'s hosts, behaves as `policy` says and asks `gate` before every
    /// request and chunk.
    pub fn new(
        allowlist: HostAllowlist,
        policy: HttpPolicy,
        gate: PermissionGate,
    ) -> PortResult<Self> {
        install_crypto_provider()?;
        let redirects = allowlist.clone();
        let max_redirects = usize::from(policy.max_redirects);
        let mut builder = Client::builder()
            .connect_timeout(Duration::from_millis(u64::from(policy.connect_timeout_ms)))
            .read_timeout(Duration::from_millis(u64::from(policy.read_timeout_ms)))
            .https_only(allowlist.require_https)
            .referer(false)
            .user_agent(concat!("Echo/", env!("CARGO_PKG_VERSION")))
            .redirect(redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= max_redirects {
                    return attempt.error("too many redirects");
                }
                let url = attempt.url();
                if redirects.allows(url.scheme(), url.host_str().unwrap_or_default()) {
                    attempt.follow()
                } else {
                    let refused = format!(
                        "redirect to a host that is not allowed: {}",
                        url.host_str().unwrap_or_default()
                    );
                    attempt.error(refused)
                }
            }));
        if !policy.system_proxy {
            builder = builder.no_proxy();
        }
        let client = builder.build().map_err(|error| {
            PortError::new(AppError::Internal).with_detail(format!(
                "the HTTP client could not be built: {}",
                describe(&error)
            ))
        })?;
        Ok(Self {
            client,
            allowlist,
            gate,
        })
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: get_from, ranged GET, resume offset, 206 Partial Content, Content-Range check
     * WHAT:  GETs `url` from byte `offset` (the whole file when 0). The body starts at `offset` when the server
     *        honoured the range (206 with a matching Content-Range) and at 0 when it sent the whole file (200);
     *        416 is RangeNotSatisfiable. Any other status, a refused host or a failed connection is `Network`.
     * WHY:   A resumed download must know where the bytes it receives belong, and a server that ignores Range must
     *        not have its bytes appended after the old ones.
     * WHERE: HttpModelStore's download of each file.
     */
    pub async fn get_from(&self, url: &str, offset: u64) -> PortResult<HttpFetch> {
        let url = self.allowed(url)?;
        self.gate.require()?;
        let mut request = self.client.get(url);
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }
        let response = request
            .send()
            .await
            .map_err(|error| network("the request failed", &error))?;
        match response.status() {
            StatusCode::OK => Ok(HttpFetch::Body(self.body(response, 0))),
            StatusCode::PARTIAL_CONTENT => {
                let start = content_range_start(&response)?;
                if start == offset {
                    Ok(HttpFetch::Body(self.body(response, start)))
                } else {
                    Err(PortError::new(AppError::Network).with_detail(format!(
                        "asked for bytes from {offset}, the server sent bytes from {start}"
                    )))
                }
            }
            StatusCode::RANGE_NOT_SATISFIABLE if offset > 0 => Ok(HttpFetch::RangeNotSatisfiable),
            status => Err(PortError::new(AppError::Network)
                .with_detail(format!("the server answered {status}"))),
        }
    }

    fn body(&self, response: Response, start: u64) -> HttpBody {
        HttpBody {
            response,
            start,
            gate: self.gate.clone(),
        }
    }

    /// `url` parsed, when its scheme and host are allowed.
    fn allowed(&self, url: &str) -> PortResult<Url> {
        let parsed = Url::parse(url).map_err(|error| {
            PortError::new(AppError::Internal).with_detail(format!("not a URL: {error}"))
        })?;
        if self
            .allowlist
            .allows(parsed.scheme(), parsed.host_str().unwrap_or_default())
        {
            Ok(parsed)
        } else {
            Err(PortError::new(AppError::Network).with_detail(format!(
                "{} is not an allowed download host",
                parsed.host_str().unwrap_or_default()
            )))
        }
    }
}

impl HttpBody {
    /// The file offset of the first byte of this body.
    pub const fn start(&self) -> u64 {
        self.start
    }

    /// The next bytes, or None at the end of the body; the gate's denial error (`Offline`) once offline mode is on.
    pub async fn chunk(&mut self) -> PortResult<Option<impl AsRef<[u8]> + use<>>> {
        self.gate.require()?;
        self.response
            .chunk()
            .await
            .map_err(|error| network("the download stopped", &error))
    }
}

/// Makes `ring` the process's rustls provider; a provider installed earlier (this or another client) is kept.
pub(super) fn install_crypto_provider() -> PortResult<()> {
    // Err means a provider is already installed, which is exactly what is needed.
    let _ = rustls::crypto::ring::default_provider().install_default();
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        Ok(())
    } else {
        Err(PortError::new(AppError::Internal).with_detail("no TLS crypto provider is installed"))
    }
}

/// The first byte offset of a 206 response's `Content-Range: bytes START-END/TOTAL`.
fn content_range_start(response: &Response) -> PortResult<u64> {
    response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_range_start)
        .ok_or_else(|| {
            PortError::new(AppError::Network)
                .with_detail("a partial response had no readable Content-Range")
        })
}

fn parse_content_range_start(value: &str) -> Option<u64> {
    let range = value.trim().strip_prefix("bytes ")?;
    let (start, _) = range.split_once('-')?;
    start.trim().parse().ok()
}

fn network(what: &str, error: &reqwest::Error) -> PortError {
    PortError::new(AppError::Network).with_detail(format!("{what}: {}", describe(error)))
}

/// An error and its causes, without the request URL (CDN URLs carry signed query strings).
fn describe(error: &reqwest::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    match error.url() {
        Some(url) => text.replace(url.as_str(), url.host_str().unwrap_or_default()),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_range_start_is_read_from_the_header_value() {
        assert_eq!(parse_content_range_start("bytes 100-199/200"), Some(100));
        assert_eq!(parse_content_range_start(" bytes 0-0/1 "), Some(0));
        assert_eq!(parse_content_range_start("bytes */200"), None);
        assert_eq!(parse_content_range_start("items 1-2/3"), None);
    }
}
