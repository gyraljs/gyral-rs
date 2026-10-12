//! Pure rendering contract; no JavaScript runtime, HTTP framework, clock, or filesystem.
//!
//! Requests become valid before they reach the renderer:
//!
//! ```
//! use gyral_renderer_domain::{ContractError, PageId, RenderRequest};
//! use serde_json::json;
//! # fn main() -> Result<(), ContractError> {
//! let page = PageId::try_from("home".to_owned())?;
//! let request = RenderRequest::try_new(page, json!({"name": "friend"}))?;
//! assert_eq!(request.page().as_str(), "home");
//! assert_eq!(PageId::try_from("../private".to_owned()), Err(ContractError::InvalidPage));
//! # Ok(())
//! # }
//! ```
#![forbid(unsafe_code)]

mod limits;
pub use limits::{LimitsError, RenderLimits};
mod manifest;
pub use manifest::{BUNDLE_ABI_VERSION, BundleManifest, ContentHash, GYRAL_VERSION, ManifestError};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use thiserror::Error;

/// Invalid values at the Rust/JavaScript boundary.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContractError {
    /// A page identifier must use 1–64 ASCII letters, digits, hyphens, or underscores.
    #[error("page identifiers must contain 1–64 ASCII letters, digits, hyphens, or underscores")]
    InvalidPage,
    /// Props must be a JSON object.
    #[error("render props must be a JSON object")]
    InvalidProps,
    /// Serialized props exceeded the scaffold's size limit.
    #[error("render props exceed the 256 KiB limit")]
    PropsTooLarge,
    /// The renderer returned no HTML.
    #[error("the renderer returned empty HTML")]
    EmptyHtml,
    /// The returned HTML exceeded the scaffold's size limit.
    #[error("rendered HTML exceeds the 2 MiB limit")]
    HtmlTooLarge,
}

/// Validated logical page name, independent of HTTP paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PageId(String);

impl TryFrom<String> for PageId {
    type Error = ContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(ContractError::InvalidPage);
        }
        Ok(Self(value))
    }
}

impl From<PageId> for String {
    fn from(page: PageId) -> Self {
        page.0
    }
}

impl PageId {
    /// Logical page name supplied to the app bundle.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRenderRequest {
    page: PageId,
    props: Value,
}

/// Permission-filtered data for one render. Debug output deliberately omits props.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawRenderRequest")]
pub struct RenderRequest {
    page: PageId,
    props: Value,
}

impl fmt::Debug for RenderRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenderRequest")
            .field("page", &self.page)
            .finish_non_exhaustive()
    }
}

impl TryFrom<RawRenderRequest> for RenderRequest {
    type Error = ContractError;

    fn try_from(raw: RawRenderRequest) -> Result<Self, Self::Error> {
        Self::try_new(raw.page, raw.props)
    }
}

impl RenderRequest {
    /// Construct a request from an already parsed page and JSON object.
    ///
    /// # Errors
    /// Returns `InvalidProps` for non-object JSON and `PropsTooLarge` above 256 KiB.
    pub fn try_new(page: PageId, props: Value) -> Result<Self, ContractError> {
        if !props.is_object() {
            return Err(ContractError::InvalidProps);
        }
        if props.to_string().len() > 256 * 1024 {
            return Err(ContractError::PropsTooLarge);
        }
        Ok(Self { page, props })
    }

    /// Page selected by the host application.
    #[must_use]
    pub fn page(&self) -> &PageId {
        &self.page
    }
}

/// Nonempty HTML produced by a trusted bundle. This type does not sanitize HTML.
#[derive(Clone, PartialEq, Eq)]
pub struct RenderedHtml(String);

impl fmt::Debug for RenderedHtml {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenderedHtml")
            .field("bytes", &self.0.len())
            .finish()
    }
}

impl TryFrom<String> for RenderedHtml {
    type Error = ContractError;

    fn try_from(html: String) -> Result<Self, Self::Error> {
        if html.is_empty() {
            return Err(ContractError::EmptyHtml);
        }
        if html.len() > Self::MAX_BYTES {
            return Err(ContractError::HtmlTooLarge);
        }
        Ok(Self(html))
    }
}

impl RenderedHtml {
    /// Maximum output size, also used before copying a V8 string into Rust.
    pub const MAX_BYTES: usize = 2 * 1024 * 1024;

    /// Read the rendered document or fragment.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Transfer the HTML into an HTTP response or static file.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

/// Compiles the crate README examples as documentation tests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
