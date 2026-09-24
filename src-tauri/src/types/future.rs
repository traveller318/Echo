/*!
 * SOURCE OF TRUTH KEYWORDS: BoxFuture, async port, dyn-compatible async, boxed future, Send future, async trait object
 * WHAT:  BoxFuture<'a, T>: a pinned, boxed, `Send` future. The return type of every async port method.
 * WHY:   Ports are held as `Arc<dyn Port>`, and an `async fn` in a trait is not dyn-compatible. Returning a boxed
 *        future keeps the traits object-safe with the standard library alone (no async-trait crate). Only the
 *        ports that wait on I/O are async (TextPolisher, ModelStore, Updater); compute- and OS-bound ports stay
 *        synchronous and the pipeline runs them on their own threads (05 decision log). Dropping the future
 *        cancels the call, which is how the pipeline applies timeouts (02 §8.3) and cancels downloads.
 * WHERE: ports/polish.rs, ports/model_store.rs, ports/updater.rs; built with `Box::pin(async move { … })` by
 *        adapters and ports/fakes; awaited by pipeline/ and ipc/.
 */

use std::{future::Future, pin::Pin};

/// A boxed `Send` future borrowing for `'a`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
