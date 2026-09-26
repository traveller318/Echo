/*!
 * SOURCE OF TRUTH KEYWORDS: net adapters, HttpClient, HttpModelStore, LoopbackClient, allowlisted HTTP, model downloads, sidecar HTTP
 * WHAT:  Network adapters: the one allowlisted HttpClient, the HttpModelStore built on it, and the LoopbackClient
 *        that talks to sidecars Echo started on 127.0.0.1.
 * WHY:   02 §10: every outbound request lives in this folder and nowhere else, so the allowlist and the offline gate
 *        cannot be bypassed by another module opening its own connection. Loopback traffic never leaves the machine,
 *        so it is not gated by offline mode, but it lives here too so every socket Echo opens is in one folder.
 * WHERE: Constructed by app/bootstrap; the store is used only through `dyn ModelStore`; the loopback client by
 *        adapters/polish/llama_server.
 */

mod http_client;
mod loopback;
mod model_store;

pub use http_client::{HttpBody, HttpClient, HttpFetch};
pub use loopback::{LoopbackClient, LoopbackResponse, free_port};
pub use model_store::HttpModelStore;
