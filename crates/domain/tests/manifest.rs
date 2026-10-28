//! Manifest boundaries and hash representation properties.
#![allow(
    clippy::expect_used,
    reason = "test constructors carry explicit fixture invariants"
)]
use gyral_renderer_domain::{
    BUNDLE_ABI_VERSION, BundleManifest, ContentHash, GYRAL_VERSION, ManifestError,
};
use proptest::prelude::*;
use serde_json::{Value, json};

fn set(value: &mut Value, path: &[&str], replacement: Value) {
    let (last, parents) = path
        .split_last()
        .expect("invariant: fixture paths are nonempty");
    let mut current = value;
    for parent in parents {
        current = current
            .get_mut(*parent)
            .expect("invariant: fixture parent exists");
    }
    current
        .as_object_mut()
        .expect("invariant: fixture field belongs to an object")
        .insert((*last).to_owned(), replacement);
}

fn raw(server: ContentHash, client: ContentHash, styles: &[ContentHash]) -> Value {
    json!({
        "abi": 1, "gyral": "0.3.7", "server_sha256": server.hex(),
        "client": {"path": format!("/assets/client.{}.js", client.hex()), "sha256": client.hex()},
        "style_hashes": styles.iter().map(|hash| hash.hex()).collect::<Vec<_>>(),
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x0061_7373_6574),
        ..ProptestConfig::default()
    })]
    #[test]
    fn hash_bytes_hex_and_json_round_trip(bytes in any::<[u8;32]>()) {
        let hash = ContentHash::from_digest(bytes);
        prop_assert_eq!(hash.hex().len(), 64);
        prop_assert_eq!(ContentHash::try_from(hash.hex()).expect("invariant: hex is canonical"), hash);
        let encoded = serde_json::to_string(&hash).expect("invariant: a hash serializes");
        prop_assert_eq!(serde_json::from_str::<ContentHash>(&encoded).expect("invariant: serialized hash is valid"), hash);
    }

    #[test]
    fn manifests_preserve_valid_metadata(
        server in any::<[u8;32]>(), client in any::<[u8;32]>(),
        styles in prop::collection::vec(any::<[u8;32]>(), 0..=32),
    ) {
        let server = ContentHash::from_digest(server);
        let client = ContentHash::from_digest(client);
        let styles = styles.into_iter().map(ContentHash::from_digest).collect::<Vec<_>>();
        let value = raw(server, client, &styles).to_string();
        let manifest = BundleManifest::try_from(value.as_str()).expect("invariant: fixture metadata is valid");
        prop_assert_eq!(manifest.server_hash(), server);
        prop_assert_eq!(manifest.client_hash(), client);
        prop_assert_eq!(manifest.client_path(), format!("/assets/client.{}.js", client.hex()));
        prop_assert_eq!(manifest.style_hashes(), styles.as_slice());
        prop_assert_eq!(serde_json::from_str::<BundleManifest>(&value).expect("invariant: serde uses the validated boundary"), manifest);
    }
}

#[test]
fn only_canonical_hashes_can_be_constructed_and_integrity_uses_standard_base64() {
    let zero = ContentHash::from_digest([0; 32]);
    assert_eq!(zero.hex(), "0".repeat(64));
    assert_eq!(zero.integrity(), format!("sha256-{}=", "A".repeat(43)));
    let high = ContentHash::from_digest([255; 32]);
    assert_eq!(high.hex(), "f".repeat(64));
    assert_eq!(high.integrity(), format!("sha256-{}8=", "/".repeat(42)));
    for invalid in [
        String::new(),
        "0".repeat(63),
        "0".repeat(65),
        "F".repeat(64),
        "g".repeat(64),
        "é".repeat(32),
        "\n".repeat(64),
    ] {
        assert!(matches!(
            ContentHash::try_from(invalid),
            Err(ManifestError::InvalidHash)
        ));
    }
}

#[test]
fn manifest_rejects_versions_paths_unknown_fields_and_oversized_headers() {
    assert_eq!(BundleManifest::MAX_BYTES, 16 * 1024);
    assert_eq!(BUNDLE_ABI_VERSION, 1);
    assert_eq!(GYRAL_VERSION, "0.3.7");
    let hash = ContentHash::from_digest([0; 32]);
    let valid = raw(hash, hash, &[]);
    for abi in [0, 2, u32::MAX] {
        let mut value = valid.clone();
        set(&mut value, &["abi"], json!(abi));
        assert!(
            matches!(BundleManifest::try_from(value.to_string().as_str()), Err(ManifestError::UnsupportedAbi {received}) if received == abi)
        );
    }
    let mut value = valid.clone();
    set(&mut value, &["gyral"], json!("0.3.1-next.8"));
    assert!(
        matches!(BundleManifest::try_from(value.to_string().as_str()), Err(ManifestError::UnsupportedGyral {received}) if received == "0.3.1-next.8")
    );
    for path in [
        "/assets/client.js",
        "//elsewhere/client.js",
        "https://elsewhere/client.js",
        "/assets/../server.js",
        "/assets/%2e%2e/server.js",
        "/assets/client.js?x=1",
    ] {
        let mut value = valid.clone();
        set(&mut value, &["client", "path"], json!(path));
        assert!(matches!(
            BundleManifest::try_from(value.to_string().as_str()),
            Err(ManifestError::InvalidClientPath)
        ));
        assert!(serde_json::from_value::<BundleManifest>(value).is_err());
    }
    for value in [
        json!({}),
        json!({"unknown":1}),
        {
            let mut v = valid.clone();
            set(&mut v, &["extra"], json!(1));
            v
        },
        {
            let mut v = valid.clone();
            set(&mut v, &["client", "extra"], json!(1));
            v
        },
        {
            let mut v = valid.clone();
            set(&mut v, &["client", "sha256"], json!("bad"));
            v
        },
    ] {
        assert!(matches!(
            BundleManifest::try_from(value.to_string().as_str()),
            Err(ManifestError::Json(_))
        ));
    }
    assert!(
        BundleManifest::try_from(raw(hash, hash, &vec![hash; 32]).to_string().as_str()).is_ok()
    );
    assert!(matches!(
        BundleManifest::try_from(raw(hash, hash, &vec![hash; 33]).to_string().as_str()),
        Err(ManifestError::TooManyStyles)
    ));
    let text = valid.to_string();
    let boundary = format!(
        "{text}{}",
        " ".repeat(BundleManifest::MAX_BYTES - text.len())
    );
    assert!(BundleManifest::try_from(boundary.as_str()).is_ok());
    assert!(matches!(
        BundleManifest::try_from(format!("{boundary} ").as_str()),
        Err(ManifestError::TooLarge)
    ));
}
