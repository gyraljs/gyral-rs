//! Manifest boundaries and hash representation properties.
#![allow(
    clippy::expect_used,
    reason = "test constructors carry explicit fixture invariants"
)]
use gyral_renderer_domain::{
    BUNDLE_ABI_VERSION, BundleManifest, ContentHash, GyralVersion, ManifestError,
    SUPPORTED_PROTOCOLS, SsrProtocol,
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
        "abi": 1, "gyral": "0.3.9", "server_sha256": server.hex(),
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
        prop_assert_eq!(manifest.protocol(), SsrProtocol::V1);
        prop_assert_eq!(manifest.gyral_version(), GyralVersion::new(0, 3, 9));
        prop_assert_eq!(serde_json::from_str::<BundleManifest>(&value).expect("invariant: serde uses the validated boundary"), manifest);
    }

    #[test]
    fn protocol_1_accepts_a_stable_release_exactly_when_it_is_at_least_0_3_8(
        major in 0_u64..3, minor in 0_u64..12, patch in 0_u64..12,
    ) {
        let release = GyralVersion::new(major, minor, patch);
        prop_assert_eq!(GyralVersion::try_from(release.to_string().as_str()).expect("invariant: display is canonical"), release);
        let result = manifest(1, &release.to_string());
        if (major, minor, patch) >= (0, 3, 8) {
            prop_assert_eq!(result.expect("invariant: release meets the minimum").gyral_version(), release);
        } else {
            let too_old = matches!(
                result,
                Err(ManifestError::GyralTooOld { received, minimum, protocol })
                    if received == release && minimum == GyralVersion::new(0, 3, 8) && protocol == SsrProtocol::V1
            );
            prop_assert!(too_old, "Gyral {} must be too old for protocol 1", release);
        }
    }
}

fn manifest(abi: u32, gyral: &str) -> Result<BundleManifest, ManifestError> {
    let hash = ContentHash::from_digest([0; 32]);
    let mut value = raw(hash, hash, &[]);
    set(&mut value, &["abi"], json!(abi));
    set(&mut value, &["gyral"], json!(gyral));
    BundleManifest::try_from(value.to_string().as_str())
}

#[test]
fn protocol_1_accepts_every_stable_gyral_release_from_0_3_8() {
    // 0.3.10 and 0.10.0 sort below 0.3.8 as text, so they prove numeric comparison.
    for gyral in ["0.3.8", "0.3.9", "0.3.10", "0.4.0", "0.10.0", "1.0.0"] {
        let accepted = manifest(1, gyral).expect("invariant: protocol 1 accepts this release");
        assert_eq!(accepted.protocol(), SsrProtocol::V1);
        assert_eq!(accepted.protocol().get(), 1);
        assert_eq!(accepted.gyral_version().to_string(), gyral);
    }
    let release = manifest(1, "0.3.10")
        .expect("invariant: 0.3.10 is accepted")
        .gyral_version();
    assert_eq!(
        (release.major(), release.minor(), release.patch()),
        (0, 3, 10)
    );
}

#[test]
fn supported_protocols_are_constants_with_their_minimum_releases() {
    assert_eq!(SUPPORTED_PROTOCOLS, &[SsrProtocol::V1]);
    assert_eq!(BUNDLE_ABI_VERSION, SsrProtocol::V1.get());
    const MINIMUM: GyralVersion = SsrProtocol::V1.minimum_gyral();
    assert_eq!(MINIMUM, GyralVersion::new(0, 3, 8));
    assert_eq!(MINIMUM.to_string(), "0.3.8");
    assert_eq!(SsrProtocol::try_from(1).ok(), Some(SsrProtocol::V1));
    for unsupported in [0, 2, u32::MAX] {
        assert!(matches!(
            SsrProtocol::try_from(unsupported),
            Err(ManifestError::UnsupportedProtocol { received }) if received == unsupported
        ));
    }
}

#[test]
fn gyral_versions_must_be_stable_canonical_releases() {
    for invalid in [
        "0.3.10-next.0",
        "0.3.9-rc.1",
        "0.3.9+build.5",
        "v0.3.9",
        "0.3",
        "0.3.9.1",
        "",
        " 0.3.9",
        "0.3.9 ",
        "0.03.9",
        "+0.3.9",
        "0.3.-9",
        "0..9",
        "0.3.18446744073709551616",
    ] {
        assert!(
            matches!(
                manifest(1, invalid),
                Err(ManifestError::InvalidGyralVersion { received }) if received == invalid
            ),
            "{invalid:?} must be rejected as an invalid release"
        );
    }
    assert_eq!(
        GyralVersion::try_from("18446744073709551615.0.0")
            .expect("invariant: u64::MAX is a valid release number")
            .major(),
        u64::MAX
    );
}

#[test]
fn rejections_name_the_supported_protocol_and_minimum_release() {
    let message = |abi: u32, gyral: &str| {
        manifest(abi, gyral)
            .expect_err("invariant: the case is unsupported")
            .to_string()
    };
    for gyral in ["0.3.7", "0.2.99", "0.0.0"] {
        assert!(matches!(
            manifest(1, gyral),
            Err(ManifestError::GyralTooOld { protocol, .. }) if protocol == SsrProtocol::V1
        ));
        let text = message(1, gyral);
        assert!(text.contains(gyral), "{text}");
        assert!(
            text.contains("0.3.8") && text.contains("protocol 1"),
            "{text}"
        );
    }
    for gyral in ["0.3.10-next.0", "0.3.9-rc.1", "0.3.9+build.5"] {
        let text = message(1, gyral);
        assert!(text.contains(gyral) && text.contains("stable"), "{text}");
        assert!(text.contains("prerelease"), "{text}");
    }
    for abi in [0, 2, u32::MAX] {
        // An unknown protocol is reported even when its Gyral release is also unsupported.
        assert!(matches!(
            manifest(abi, "0.3.7"),
            Err(ManifestError::UnsupportedProtocol { received }) if received == abi
        ));
        let text = message(abi, "0.3.9");
        assert!(text.contains(&format!("protocol {abi}")), "{text}");
        assert!(text.contains("supports protocol 1"), "{text}");
        assert!(text.contains("Gyral 0.3.8 or newer"), "{text}");
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
    let hash = ContentHash::from_digest([0; 32]);
    let valid = raw(hash, hash, &[]);
    for abi in [0, 2, u32::MAX] {
        let mut value = valid.clone();
        set(&mut value, &["abi"], json!(abi));
        assert!(
            matches!(BundleManifest::try_from(value.to_string().as_str()), Err(ManifestError::UnsupportedProtocol {received}) if received == abi)
        );
    }
    let mut value = valid.clone();
    set(&mut value, &["gyral"], json!("0.3.1-next.8"));
    assert!(
        matches!(BundleManifest::try_from(value.to_string().as_str()), Err(ManifestError::InvalidGyralVersion {received}) if received == "0.3.1-next.8")
    );
    for (field, replacement) in [
        ("abi", json!("1")),
        ("abi", json!(-1)),
        ("gyral", json!(39)),
    ] {
        let mut value = valid.clone();
        set(&mut value, &[field], replacement);
        assert!(matches!(
            BundleManifest::try_from(value.to_string().as_str()),
            Err(ManifestError::Json(_))
        ));
    }
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
