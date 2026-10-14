//! CSP and immutable asset response contracts, without constructing V8.
#![allow(
    clippy::expect_used,
    reason = "fixtures establish digest and header invariants"
)]
use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};
use gyral_axum::{DocumentPolicy, RenderRejection, client_asset};
use gyral_renderer::{AppArtifacts, BundleManifest, RenderError};
use serde_json::json;

fn manifest(styles: Vec<String>) -> BundleManifest {
    // SHA-256 of the literal five bytes "hello"; neither fixture is executed.
    let hash = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    let raw = json!({"abi":1,"gyral":"0.3.5","server_sha256":hash,"client":{"path":format!("/assets/client.{hash}.js"),"sha256":hash},"style_hashes":styles});
    BundleManifest::try_from(raw.to_string().as_str())
        .expect("invariant: fixture metadata is valid")
}

#[tokio::test]
async fn verified_client_bytes_have_immutable_cache_and_javascript_type() {
    let app = AppArtifacts::try_new(manifest(vec![]), "hello".to_owned(), b"hello".to_vec())
        .expect("invariant: published hash matches hello bytes");
    let response = client_asset(&app);
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .expect("invariant: asset sets cache policy"),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .expect("invariant: asset sets MIME"),
        "text/javascript; charset=utf-8"
    );
    assert_eq!(
        response
            .headers()
            .get("x-content-type-options")
            .expect("invariant: asset disables MIME sniffing"),
        "nosniff"
    );
    assert_eq!(
        to_bytes(response.into_body(), 32)
            .await
            .expect("invariant: fixture body is bounded"),
        "hello"
    );
}

#[test]
fn restrictive_policy_covers_static_styles_and_opaque_errors_without_relaxing_scripts() {
    for count in [0, 1, 32] {
        let metadata = manifest(vec!["0".repeat(64); count]);
        let policy = DocumentPolicy::try_new(&metadata)
            .expect("invariant: validated hashes produce a legal header");
        let header = policy
            .header_value()
            .to_str()
            .expect("invariant: policy is ASCII");
        assert!(
            header.contains("script-src 'sha256-LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ='")
        );
        assert!(header.contains("default-src 'none'"));
        assert!(header.contains("script-src-attr 'none'"));
        assert!(header.contains("style-src-attr 'none'"));
        assert!(header.contains("form-action 'self'"));
        assert!(header.contains("frame-ancestors 'none'"));
        assert!(!header.contains("unsafe-inline"));
        assert!(!header.contains("unsafe-eval"));
        assert!(header.len() < 4096);
        if count == 0 {
            assert!(header.contains("style-src 'none'"));
        } else {
            assert!(header.contains(&format!("style-src 'sha256-{}='", "A".repeat(43))));
        }
        let error = policy.apply(RenderRejection::from(RenderError::Busy).into_response());
        assert_eq!(
            error.headers().get("content-security-policy"),
            Some(policy.header_value())
        );
        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            error
                .headers()
                .get("cache-control")
                .expect("invariant: errors retain private caching"),
            "no-store"
        );
    }
}
