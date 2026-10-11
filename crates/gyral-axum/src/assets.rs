use axum::{
    body::{Body, Bytes},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use gyral_renderer::{AppArtifacts, BundleManifest};

/// A document policy could not be encoded as an HTTP header.
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    /// HTTP rejected the generated policy header.
    #[error("could not encode the document CSP")]
    Header(#[from] axum::http::header::InvalidHeaderValue),
}

/// Restrictive example policy for one self-contained browser entry and static CSS.
///
/// The policy permits the verified entry by hash, component/document styles by
/// hash, same-origin connections and form actions. It denies inline scripts,
/// event handlers, style attributes, framing, and plugins. Applications with
/// other resource needs should build and own their own explicit policy.
#[derive(Debug, Clone)]
pub struct DocumentPolicy(HeaderValue);

impl DocumentPolicy {
    /// Build a bounded CSP from a validated manifest.
    ///
    /// # Errors
    /// Returns a typed header encoding error.
    pub fn try_new(manifest: &BundleManifest) -> Result<Self, PolicyError> {
        let styles = if manifest.style_hashes().is_empty() {
            "'none'".to_owned()
        } else {
            manifest
                .style_hashes()
                .iter()
                .map(|hash| format!("'{}'", hash.integrity()))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let policy = format!(
            "default-src 'none'; script-src '{}'; script-src-attr 'none'; style-src {styles}; style-src-attr 'none'; connect-src 'self'; form-action 'self'; base-uri 'none'; object-src 'none'; frame-ancestors 'none'",
            manifest.client_hash().integrity(),
        );
        Ok(Self(HeaderValue::from_str(&policy)?))
    }

    /// Apply the same policy to a successful document or an opaque rendering failure.
    #[must_use]
    pub fn apply(&self, mut response: Response) -> Response {
        response
            .headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, self.0.clone());
        response
    }

    /// Read the generated value for a host response layer.
    #[must_use]
    pub fn header_value(&self) -> &HeaderValue {
        &self.0
    }
}

/// Serve only verified browser bytes at the manifest's exact URL.
///
/// The application must mount this response at `artifacts.manifest().client_path()`.
/// Success is cached for a year as immutable; misses and errors must use no-store.
/// Bytes share immutable startup storage and do not reread mutable files.
#[must_use]
pub fn client_asset(artifacts: &AppArtifacts) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Body::from(Bytes::from_owner(artifacts.client_bytes())),
    )
        .into_response()
}
