//! A W3C WebDriver server for our from-scratch browser engine.
//!
//! This crate is a thin protocol adapter: it speaks the WebDriver HTTP/JSON wire protocol and
//! drives an [`engine::Engine`] per session. It is headless (no window) and is intended as the
//! basis for driving WPT's `wptrunner` and reftests.
//!
//! See [`server::run`] to start the server and [`server::serve`] to serve on a pre-bound listener
//! (used by the integration tests with an ephemeral port).

pub mod json;
pub mod server;
