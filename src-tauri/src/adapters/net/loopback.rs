/*!
 * SOURCE OF TRUTH KEYWORDS: LoopbackClient, loopback HTTP, 127.0.0.1 sidecar, local sidecar requests, no proxy, bearer key, llama-server HTTP
 * WHAT:  LoopbackClient: HTTP to a sidecar Echo started itself on 127.0.0.1. `status(port, path)` answers a GET's
 *        status code (a health check); `post_json(port, path, key, body)` POSTs JSON with a bearer key and returns
 *        the status and body; `free_port` finds a port a sidecar can listen on.
 * WHY:   02 §10: the one outbound client is HttpClient (allowlisted https hosts, offline gate). Talking to a sidecar
 *        is not network traffic, so it gets its own client that cannot leave the machine by construction: the URL is
 *        always `http://127.0.0.1:<port><path>` with a `'static` path, so no host or URL can be passed in; proxies
 *        are ignored (a system proxy must never see loopback traffic, which carries transcript text); redirects are
 *        refused; offline mode does not apply. Every failure carries the AppError the owner chose (`Polish` for the
 *        LLM sidecar), with the cause as log detail. Dropping a request future closes its connection, which
 *        llama-server takes as a cancel (checked 2026-09-26 on b11146, stream and non-stream).
 * WHERE: adapters/polish/llama_server (health checks, warm-up and chat completions).
 */

use std::time::Duration;

use reqwest::{
    Client,
    header::{AUTHORIZATION, CONTENT_TYPE},
    redirect,
};

use super::http_client::install_crypto_provider;
use crate::types::{AppError, PortError, PortResult};

/// How long connecting to a local port may take (a sidecar that is up accepts at once).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);

/// HTTP to sidecars on 127.0.0.1.
pub struct LoopbackClient {
    client: Client,
    failure: AppError,
}

/// A sidecar's answer.
pub struct LoopbackResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl LoopbackClient {
    /// A client whose failures are `failure` (with the cause as detail).
    pub fn new(failure: AppError) -> PortResult<Self> {
        // reqwest builds its TLS config even for plain http, and panics without a provider (05 W41).
        install_crypto_provider()?;
        let client = Client::builder()
            .no_proxy()
            .redirect(redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .referer(false)
            .user_agent(concat!("Echo/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| {
                PortError::new(AppError::Internal).with_detail(format!(
                    "the loopback HTTP client could not be built: {error}"
                ))
            })?;
        Ok(Self { client, failure })
    }

    /// The status of `GET http://127.0.0.1:<port><path>`, waiting at most `timeout`.
    pub async fn status(
        &self,
        port: u16,
        path: &'static str,
        timeout: Duration,
    ) -> PortResult<u16> {
        let response = self
            .client
            .get(url(port, path))
            .timeout(timeout)
            .send()
            .await
            .map_err(|error| self.failed("the request failed", &error))?;
        Ok(response.status().as_u16())
    }

    /// POSTs `body` (JSON) to `http://127.0.0.1:<port><path>` with `key` as bearer token; the answer's status and
    /// body. No timeout: the caller races it and dropping the future cancels it.
    pub async fn post_json(
        &self,
        port: u16,
        path: &'static str,
        key: &str,
        body: Vec<u8>,
    ) -> PortResult<LoopbackResponse> {
        let response = self
            .client
            .post(url(port, path))
            .header(CONTENT_TYPE, "application/json")
            .header(AUTHORIZATION, format!("Bearer {key}"))
            .body(body)
            .send()
            .await
            .map_err(|error| self.failed("the request failed", &error))?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|error| self.failed("the answer broke off", &error))?;
        Ok(LoopbackResponse {
            status,
            body: body.to_vec(),
        })
    }

    fn failed(&self, what: &str, error: &reqwest::Error) -> PortError {
        PortError::new(self.failure.clone()).with_detail(format!("{what}: {error}"))
    }
}

/// The only URLs this client ever requests.
fn url(port: u16, path: &'static str) -> String {
    format!("http://127.0.0.1:{port}{path}")
}

/**
 * SOURCE OF TRUTH KEYWORDS: free_port, random free port, loopback port pick, sidecar port
 * WHAT:  A port on 127.0.0.1 that the OS reports free now.
 * WHY:   05 A13: a fixed port collides with other software; binding port 0 lets Windows pick an unused one. The
 *        probe socket closes before the sidecar binds, so another program could take the port in between: the
 *        sidecar then fails to start and its supervisor starts it again on a fresh port.
 * WHERE: adapters/polish/llama_server before each start.
 */
pub fn free_port() -> PortResult<u16> {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|probe| probe.local_addr())
        .map(|address| address.port())
        .map_err(|error| {
            PortError::new(AppError::Internal)
                .with_detail(format!("no free loopback port: {error}"))
        })
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    /// Answers one connection with `answer` after reading the request; returns the port and the request text.
    fn serve_once(answer: &'static str) -> (u16, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            // Headers, then the body its Content-Length announces.
            loop {
                let read = stream.read(&mut buffer).unwrap();
                request.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&request);
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
                if read == 0 {
                    break;
                }
            }
            stream.write_all(answer.as_bytes()).unwrap();
            String::from_utf8_lossy(&request).into_owned()
        });
        (port, handle)
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn posts_json_with_the_key_to_loopback_only() {
        let (port, server) =
            serve_once("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
        let client = LoopbackClient::new(AppError::Polish).unwrap();
        let response = runtime()
            .block_on(client.post_json(port, "/v1/chat/completions", "k3y", b"{\"a\":1}".to_vec()))
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"{}");
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains(&format!("host: 127.0.0.1:{port}"))
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer k3y")
        );
        assert!(request.ends_with("{\"a\":1}"));
    }

    #[test]
    fn reports_status_and_maps_failures_to_the_owners_error() {
        let (port, server) = serve_once(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        let client = LoopbackClient::new(AppError::Polish).unwrap();
        let runtime = runtime();
        assert_eq!(
            runtime
                .block_on(client.status(port, "/health", Duration::from_secs(5)))
                .unwrap(),
            503
        );
        server.join().unwrap();
        // Nothing listens there any more.
        let refused = runtime
            .block_on(client.status(port, "/health", Duration::from_secs(5)))
            .err()
            .unwrap();
        assert_eq!(refused.error(), &AppError::Polish);
        assert!(refused.detail().is_some());
    }

    #[test]
    fn free_port_is_bindable_on_loopback() {
        let port = free_port().unwrap();
        assert_ne!(port, 0);
        assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
    }
}
