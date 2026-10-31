//! Executable contract verification.
#![allow(
    clippy::expect_used,
    reason = "test fixtures prove constructor and runtime invariants"
)]
use gyral_renderer::{AppBundle, ContractError, PageId, RenderError, RenderRequest, Renderer};
use serde_json::json;

fn request(page: &str, props: serde_json::Value) -> RenderRequest {
    RenderRequest::try_new(
        PageId::try_from(page.to_owned()).expect("invariant: test page is valid"),
        props,
    )
    .expect("invariant: test props are a small JSON object")
}

#[test]
fn embedded_rendering_preserves_data_isolates_requests_and_reports_failures() {
    let source = r#"
      globalThis.gyralBundleVersion = 1; globalThis.gyralRender = async ({page, props}) => {
        if (page === 'throw') throw new Error('private diagnostic');
        if (page === 'bad') return 42;
        if (page === 'empty') return '';
        if (page === 'huge') return 'x'.repeat(2 * 1024 * 1024 + 1);
        if (page === 'capabilities') return [typeof fetch, typeof globalThis.Deno?.readTextFile,
          new Intl.DateTimeFormat('en', {timeZone:'America/New_York', hour:'numeric', hour12:false}).format(new Date('2026-01-01T12:00:00Z'))].join('|');
        globalThis.count = (globalThis.count ?? 0) + 1;
        return JSON.stringify({count:globalThis.count, props});
      };
    "#;
    let bundle = AppBundle::try_from(source.to_owned())
        .expect("invariant: fixture bundle is nonempty and small");
    let (renderer, worker) =
        Renderer::start(bundle).expect("invariant: test environment supports renderer threads");
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("invariant: test environment supports Tokio");
    executor.block_on(async {
        let props =
            json!({"name":"\"; throw new Error('injected'); // 🏀", "__proto__":{"value":1}});
        for _ in 0..2 {
            let output = renderer
                .render(request("home", props.clone()))
                .await
                .expect("invariant: fixture resolves to a string");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(output.as_str())
                    .expect("invariant: fixture emits JSON"),
                json!({"count":1,"props":props})
            );
        }
        assert_eq!(
            renderer
                .render(request("capabilities", json!({})))
                .await
                .expect("invariant: ICU data is embedded")
                .as_str(),
            "undefined|undefined|07"
        );
        assert!(matches!(
            renderer.render(request("throw", json!({}))).await,
            Err(RenderError::JavaScript { .. })
        ));
        assert!(matches!(
            renderer.render(request("bad", json!({}))).await,
            Err(RenderError::InvalidResult)
        ));
        assert!(matches!(
            renderer.render(request("empty", json!({}))).await,
            Err(RenderError::Contract(ContractError::EmptyHtml))
        ));
        assert!(matches!(
            renderer.render(request("huge", json!({}))).await,
            Err(RenderError::Contract(ContractError::HtmlTooLarge))
        ));
        worker
            .shutdown()
            .await
            .expect("invariant: completed worker can be joined");
        assert!(matches!(
            renderer.render(request("home", json!({}))).await,
            Err(RenderError::Unavailable)
        ));
    });
}

#[test]
fn bundle_parser_rejects_empty_and_oversized_source() {
    assert!(matches!(
        AppBundle::try_from("  ".to_owned()),
        Err(gyral_renderer::BundleError::Empty)
    ));
    assert!(matches!(
        AppBundle::try_from("x".repeat(8 * 1024 * 1024 + 1)),
        Err(gyral_renderer::BundleError::TooLarge)
    ));
}

/// Protocol 1's host environment (docs/design/bundle-contract.md). A runtime upgrade that
/// drops a promised global breaks bundles; one that adds a global changes the documented
/// profile. Either needs a reviewed contract change.
#[test]
fn protocol_1_host_environment_matches_the_documented_profile() {
    let source = r#"
      globalThis.gyralBundleVersion = 1;
      globalThis.gyralRender = () => {
        const promised = ['queueMicrotask', 'console', 'Intl', 'Temporal', 'WebAssembly']
          .filter(name => typeof globalThis[name] === 'undefined');
        const absent = ['TextEncoder', 'TextDecoder', 'btoa', 'atob', 'setTimeout', 'setInterval',
          'fetch', 'Response', 'Request', 'Headers', 'URL', 'URLSearchParams', 'crypto',
          'structuredClone', 'performance', 'AbortController', 'EventTarget', 'Blob',
          'window', 'self', 'document', 'navigator', 'process', 'require']
          .filter(name => typeof globalThis[name] !== 'undefined');
        return JSON.stringify({ promised, absent, deno: Object.keys(globalThis.Deno ?? {}) });
      };
    "#;
    let bundle = AppBundle::try_from(source.to_owned()).expect("invariant: the probe is bounded");
    let (renderer, worker) =
        Renderer::start(bundle).expect("invariant: test environment supports renderer threads");
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("invariant: test environment supports Tokio");
    executor.block_on(async {
        let output = renderer
            .render(request("profile", json!({})))
            .await
            .expect("invariant: the probe returns a string");
        // Missing promised globals and present unpromised ones are both listed here.
        // deno_core's internal `Deno.core` exists but is not part of the protocol.
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(output.as_str())
                .expect("invariant: the probe emits JSON"),
            json!({"promised": [], "absent": [], "deno": ["core"]})
        );
        worker
            .shutdown()
            .await
            .expect("invariant: completed worker can be joined");
    });
}
