//! Executable contract verification.
#![allow(
    clippy::expect_used,
    reason = "test fixtures prove build and runtime invariants"
)]
use gyral_renderer::{
    AppArtifacts, BundleManifest, ContentHash, PageId, RenderError, RenderRequest, Renderer,
};
use serde_json::json;
use sha2::{Digest, Sha256};

fn artifacts() -> AppArtifacts {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/axum-app/dist");
    let metadata = std::fs::read_to_string(root.join("manifest.json"))
        .expect("invariant: run pnpm build:example before integration tests");
    let manifest =
        BundleManifest::try_from(metadata.as_str()).expect("invariant: build metadata uses ABI 1");
    let source = std::fs::read_to_string(root.join("server.js"))
        .expect("invariant: the build emits server source");
    let client = std::fs::read(
        root.join("assets")
            .join(format!("client.{}.js", manifest.client_hash().hex())),
    )
    .expect("invariant: the build emits the hashed browser entry");
    AppArtifacts::try_new(manifest, source, client)
        .expect("invariant: build files belong to the same manifest")
}

#[test]
fn actual_gyral_bundle_renders_components_and_hydration_seeds() {
    let app = artifacts();
    let (renderer, worker) = Renderer::start(app.bundle().clone())
        .expect("invariant: test environment supports renderer threads");
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("invariant: test environment supports Tokio");
    executor.block_on(async {
        let request = RenderRequest::try_new(
            PageId::try_from("home".to_owned()).expect("invariant: home is valid"),
            json!({"name":"<script>bad()</script> 🏀"}),
        )
        .expect("invariant: test props are valid");
        let output = renderer
            .render(request)
            .await
            .expect("invariant: exact 0.3.2 fixture renders in deno_core");
        assert!(
            output
                .as_str()
                .contains("&lt;script&gt;bad()&lt;/script&gt; 🏀")
        );
        assert!(output.as_str().contains("shadowrootmode=\"open\""));
        assert!(output.as_str().contains("data-gyral-seed"));
        assert!(!output.as_str().contains("data-gyral-error"));
        assert!(output.as_str().contains(app.manifest().client_path()));
        assert!(
            output
                .as_str()
                .contains(&app.manifest().client_hash().integrity())
        );
        // Check the emitted style texts, not just a separately registered build hash.
        for text in output
            .as_str()
            .split("<style>")
            .skip(1)
            .filter_map(|part| part.split_once("</style>").map(|(css, _)| css))
        {
            let hash = ContentHash::from_digest(Sha256::digest(text.as_bytes()).into());
            assert!(app.manifest().style_hashes().contains(&hash));
        }
        worker
            .shutdown()
            .await
            .expect("invariant: completed worker can be joined");
    });
}

#[test]
fn actual_gyral_stores_timezones_and_errors_preserve_request_isolation() {
    let app = artifacts();
    let (renderer, worker) =
        Renderer::start(app.bundle().clone()).expect("invariant: environment supports embedded V8");
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("invariant: environment supports Tokio");
    executor.block_on(async {
        for (name, visits, instant, zone) in [
            ("First visitor", 23, "2026-11-01T05:30:00Z", "EDT"),
            ("Second visitor", 7, "2026-11-01T06:30:00Z", "EST"),
        ] {
            let request = RenderRequest::try_new(
                PageId::try_from("home".to_owned()).expect("invariant: home is valid"),
                json!({"name":name,"visits":visits,"instant":instant}),
            )
            .expect("invariant: fixture DTO is bounded");
            let output = renderer
                .render(request)
                .await
                .expect("invariant: real Gyral fixture renders stores and timezones");
            let html = output.as_str();
            assert!(html.contains("data-gyral-stores"));
            assert!(html.contains(name));
            assert!(html.contains(&format!("&quot;count&quot;:{visits}")));
            assert!(html.contains(zone));
            assert!(html.contains("1:30"));
            assert!(!html.contains("unseeded"));
            assert!(!html.contains("data-gyral-error"));
            if name == "Second visitor" {
                assert!(!html.contains("First visitor"));
            }
        }
        let request = RenderRequest::try_new(
            PageId::try_from("broken".to_owned()).expect("invariant: broken is valid"),
            json!({}),
        )
        .expect("invariant: fixture DTO is valid");
        assert!(matches!(
            renderer.render(request).await,
            Err(RenderError::JavaScript { .. })
        ));
        let request = RenderRequest::try_new(
            PageId::try_from("home".to_owned()).expect("invariant: home is valid"),
            json!({"name":"After error"}),
        )
        .expect("invariant: fixture DTO is valid");
        assert!(
            renderer
                .render(request)
                .await
                .expect("invariant: fresh isolate recovers after real Gyral error")
                .as_str()
                .contains("After error")
        );
        worker
            .shutdown()
            .await
            .expect("invariant: the worker finishes cleanup");
    });
}
