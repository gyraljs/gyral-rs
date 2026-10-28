//! Byte-pair integrity and bundle ABI rejection.
#![allow(
    clippy::expect_used,
    reason = "fixtures establish byte and runtime invariants"
)]
use gyral_renderer::{
    AppArtifacts, AppBundle, ArtifactError, BundleManifest, ContentHash, PageId, RenderError,
    RenderRequest, Renderer,
};
use serde_json::json;
use sha2::{Digest, Sha256};

fn manifest(source: &str, client: &[u8]) -> BundleManifest {
    let hash = |bytes: &[u8]| ContentHash::from_digest(Sha256::digest(bytes).into()).hex();
    let client = hash(client);
    let raw = json!({"abi":1,"gyral":"0.3.7","server_sha256":hash(source.as_bytes()),"client":{"path":format!("/assets/client.{client}.js"),"sha256":client},"style_hashes":[]});
    BundleManifest::try_from(raw.to_string().as_str())
        .expect("invariant: generated fixture is a valid manifest")
}

#[test]
fn artifacts_bind_both_byte_sequences_and_reject_mixed_builds() {
    let source =
        "globalThis.gyralBundleVersion = 1; globalThis.gyralRender = () => '<p>ready</p>';";
    let client = b"export const ready = true;";
    let metadata = manifest(source, client);
    let app = AppArtifacts::try_new(metadata.clone(), source.to_owned(), client.to_vec())
        .expect("invariant: byte-identical artifacts verify");
    assert_eq!(app.manifest(), &metadata);
    assert_eq!(app.client_bytes().as_ref(), client);
    assert!(!format!("{app:?}").contains("export const ready"));
    assert!(
        matches!(AppArtifacts::try_new(metadata.clone(), format!("{source} "), client.to_vec()), Err(ArtifactError::ServerDigest {expected,actual}) if expected == metadata.server_hash() && expected != actual)
    );
    assert!(
        matches!(AppArtifacts::try_new(metadata.clone(), source.to_owned(), b"wrong build".to_vec()), Err(ArtifactError::ClientDigest {expected,actual}) if expected == metadata.client_hash() && expected != actual)
    );
    assert!(matches!(
        AppArtifacts::try_new(metadata.clone(), " ".to_owned(), client.to_vec()),
        Err(ArtifactError::Bundle(gyral_renderer::BundleError::Empty))
    ));
    assert!(matches!(
        AppArtifacts::try_new(metadata.clone(), source.to_owned(), vec![]),
        Err(ArtifactError::ClientSize)
    ));
    assert!(matches!(
        AppArtifacts::try_new(
            metadata,
            source.to_owned(),
            vec![0; AppArtifacts::MAX_CLIENT_BYTES + 1]
        ),
        Err(ArtifactError::ClientSize)
    ));
    let max = vec![0; AppArtifacts::MAX_CLIENT_BYTES];
    assert!(AppArtifacts::try_new(manifest(source, &max), source.to_owned(), max).is_ok());
}

#[test]
fn unsupported_or_absent_bundle_versions_fail_before_invoking_app_code() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("invariant: test environment supports Tokio");
    executor.block_on(async {
        for (declaration, expected) in [("",None),("globalThis.gyralBundleVersion = 2;",Some(2)),("globalThis.gyralBundleVersion = 0;",Some(0)),("globalThis.gyralBundleVersion = '1';",None),("globalThis.gyralBundleVersion = -1;",None),("globalThis.gyralBundleVersion = 1.5;",None),("globalThis.gyralBundleVersion = 4294967296;",None)] {
            let source = format!("{declaration} globalThis.gyralRender = () => {{ throw new Error('entrypoint must not run'); }};");
            let (renderer, worker) = Renderer::start(AppBundle::try_from(source).expect("invariant: fixture source is bounded")).expect("invariant: environment supports native threads");
            let request = RenderRequest::try_new(PageId::try_from("home".to_owned()).expect("invariant: home is valid"), json!({})).expect("invariant: fixture props are valid");
            assert!(matches!(renderer.render(request).await, Err(RenderError::UnsupportedBundleAbi {received}) if received == expected));
            worker.shutdown().await.expect("invariant: rejected bundle still shuts down");
        }
    });
}
