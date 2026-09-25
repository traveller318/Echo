/*!
 * SOURCE OF TRUTH KEYWORDS: net adapters, HttpClient, HttpModelStore, allowlisted HTTP, model downloads
 * WHAT:  Network adapters: the one allowlisted HttpClient and the HttpModelStore built on it.
 * WHY:   02 §10: every outbound request lives in this folder and nowhere else, so the allowlist and the offline gate
 *        cannot be bypassed by another module opening its own connection.
 * WHERE: Constructed by app/bootstrap; the store is used only through `dyn ModelStore`.
 */

mod http_client;
mod model_store;

pub use http_client::{HttpBody, HttpClient, HttpFetch};
pub use model_store::HttpModelStore;
