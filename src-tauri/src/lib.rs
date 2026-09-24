/*!
 * SOURCE OF TRUTH KEYWORDS: lib.rs, crate root, module declarations, layers, echo_lib
 * WHAT:  Crate root of `echo_lib`: declares the eight layer modules and nothing else.
 * WHY:   Layers are folders under one crate; imports between them may only point downward per the matrix in
 *        docs/02 §3.2 (enforced by tests/architecture.rs). Logic never lives here.
 * WHERE: Linked by main.rs (`echo_lib::app::run`) and by integration tests.
 */

pub mod adapters;
pub mod app;
pub mod ipc;
pub mod pipeline;
pub mod ports;
pub mod registry;
pub mod services;
pub mod types;
