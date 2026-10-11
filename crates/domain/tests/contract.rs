//! Executable contract verification.
#![allow(
    clippy::expect_used,
    reason = "test fixtures prove constructor invariants"
)]
use gyral_renderer_domain::{ContractError, PageId, RenderRequest, RenderedHtml};
use proptest::prelude::*;
use serde_json::json;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x0067_7972_616c),
        ..ProptestConfig::default()
    })]
    #[test]
    fn valid_requests_round_trip(page in "[a-zA-Z0-9_-]{1,64}", name in ".{0,512}") {
        let request = RenderRequest::try_new(PageId::try_from(page.clone())?, json!({"name": name}))?;
        prop_assert_eq!(request.page().as_str(), page);
        let encoded = serde_json::to_string(&request)?;
        prop_assert_eq!(serde_json::from_str::<RenderRequest>(&encoded)?, request);
    }

    #[test]
    fn rendered_output_is_preserved(html in ".{1,512}") {
        let rendered = RenderedHtml::try_from(html.clone())?;
        prop_assert_eq!(rendered.as_str(), &html);
        prop_assert_eq!(rendered.into_string(), html);
    }

    #[test]
    fn page_identifiers_cannot_be_paths(prefix in "[a-zA-Z0-9_-]{0,20}", suffix in "[a-zA-Z0-9_-]{0,20}") {
        prop_assert_eq!(PageId::try_from(format!("{prefix}/{suffix}")), Err(ContractError::InvalidPage));
        prop_assert_eq!(PageId::try_from(format!("{prefix}\\{suffix}")), Err(ContractError::InvalidPage));
    }
}

#[test]
fn rejects_bad_shapes_unknown_fields_and_oversized_data() {
    for page in [
        String::new(),
        "a".repeat(65),
        "café".to_owned(),
        "x\ny".to_owned(),
    ] {
        assert_eq!(PageId::try_from(page), Err(ContractError::InvalidPage));
    }
    assert!(serde_json::from_value::<RenderRequest>(json!({"page":"home","props":[]})).is_err());
    assert!(
        serde_json::from_value::<RenderRequest>(json!({"page":"home","props":{},"extra":1}))
            .is_err()
    );
    let page = PageId::try_from("home".to_owned()).expect("invariant: home is a valid page");
    assert_eq!(
        RenderRequest::try_new(page, json!({"value":"x".repeat(256 * 1024)})).err(),
        Some(ContractError::PropsTooLarge)
    );
    assert_eq!(
        RenderedHtml::try_from(String::new()),
        Err(ContractError::EmptyHtml)
    );
    assert_eq!(
        RenderedHtml::try_from("x".repeat(2 * 1024 * 1024 + 1)),
        Err(ContractError::HtmlTooLarge)
    );
}

#[test]
fn documented_size_limits_include_the_boundary() -> Result<(), ContractError> {
    let page = PageId::try_from("a".repeat(64))?;
    let props = json!({"value": "x".repeat(256 * 1024 - 12)});
    assert_eq!(props.to_string().len(), 256 * 1024);
    let request = RenderRequest::try_new(page, props)?;
    assert_eq!(request.page().as_str(), "a".repeat(64));
    let html = "x".repeat(2 * 1024 * 1024);
    assert_eq!(RenderedHtml::try_from(html.clone())?.into_string(), html);
    Ok(())
}

#[test]
fn debug_output_omits_user_data_and_html() {
    let page = PageId::try_from("home".to_owned()).expect("invariant: home is a valid page");
    let request = RenderRequest::try_new(page, json!({"private":"keep-private"}))
        .expect("invariant: fixture is a small object");
    let html = RenderedHtml::try_from("<p>keep-private</p>".to_owned())
        .expect("invariant: fixture is small nonempty HTML");
    let request_debug = format!("{request:?}");
    let html_debug = format!("{html:?}");
    assert!(!request_debug.contains("keep-private"));
    assert!(!html_debug.contains("keep-private"));
    assert!(request_debug.contains("home"));
    assert!(html_debug.contains("bytes: 19"));
}
