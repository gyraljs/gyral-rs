//! Server-render [Gyral](https://gyral.dev) components in a Rust process, then let
//! Gyral in the browser hydrate the same HTML.
//!
//! This is the gyral-rs front door. Its root re-exports the
//! [`gyral-renderer`](https://docs.rs/gyral-renderer) API, which in turn re-exports
//! the validated request, limit, manifest, and HTML types from
//! [`gyral-renderer-domain`](https://docs.rs/gyral-renderer-domain). The `axum`
//! feature adds the [`gyral-axum`](https://docs.rs/gyral-axum) response adapter as
//! `gyral::axum`.
//!
//! Your application keeps routes, authorization, data loading, and mutations. It
//! passes a validated page name and browser-safe JSON to the [`Renderer`]:
//!
//! ```
//! use gyral::{ContractError, PageId, RenderRequest};
//! use serde_json::json;
//! # fn main() -> Result<(), ContractError> {
//! let page = PageId::try_from("home".to_owned())?;
//! let request = RenderRequest::try_new(page, json!({"name": "friend"}))?;
//! assert_eq!(request.page().as_str(), "home");
//! # Ok(())
//! # }
//! ```
//!
//! With the `axum` feature, a handler returns the adapter's response types, which
//! apply private caching and replace rendering failures with fixed bodies:
//!
//! ```no_run
//! # #[cfg(feature = "axum")]
//! # mod handler {
//! use gyral::axum::{RenderRejection, RenderedPage, render_page};
//! use gyral::{RenderRequest, Renderer};
//!
//! pub async fn home(
//!     renderer: &Renderer,
//!     request: RenderRequest,
//! ) -> Result<RenderedPage, RenderRejection> {
//!     render_page(renderer, request).await
//! }
//! # }
//! ```
//!
//! Building this crate links V8 through `deno_core`. Pin the native archive as
//! described in the [`gyral-renderer` guide](https://github.com/gyraljs/gyral-rs/blob/main/crates/gyral-renderer/README.md#pin-the-native-v8-archive).
//! The renderer accepts trusted, reviewed bundles; it is not a sandbox for
//! untrusted JavaScript.
#![forbid(unsafe_code)]

pub use gyral_renderer::*;

/// Axum responses for rendered pages and verified browser assets.
///
/// Available with the `axum` feature. Applications still own routes and access checks.
#[cfg(feature = "axum")]
pub use gyral_axum as axum;

/// Compiles the crate README examples as documentation tests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
