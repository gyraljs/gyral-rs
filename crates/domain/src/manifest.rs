use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Newest SSR protocol this renderer supports: the `gyralBundleVersion` a bundle
/// written for it declares. [`SUPPORTED_PROTOCOLS`] lists every supported protocol.
pub const BUNDLE_ABI_VERSION: u32 = SsrProtocol::V1.get();

/// SSR protocols this renderer accepts, oldest first, each with its minimum Gyral release.
pub const SUPPORTED_PROTOCOLS: &[SsrProtocol] = &[SsrProtocol::V1];

/// Where the supported protocols and the gyral-rs releases that accept them are listed.
const PROTOCOLS_URL: &str = "https://github.com/gyraljs/gyral-rs/blob/main/docs/design/bundle-contract.md#supported-protocols";

/// Invalid build metadata. Digest mismatches in actual files are renderer errors.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// The metadata file exceeded its byte bound before decoding.
    #[error("the manifest exceeds 16 KiB")]
    TooLarge,
    /// Malformed JSON, unknown fields, or an invalid nested value.
    #[error("invalid bundle manifest JSON")]
    Json(#[source] serde_json::Error),
    /// A SHA-256 digest must use exactly 64 lowercase hexadecimal characters.
    #[error("a content hash must be 64 lowercase hexadecimal characters")]
    InvalidHash,
    /// The manifest's `abi` names an SSR protocol this renderer does not support.
    #[error(
        "the bundle uses SSR protocol {received} (the manifest's `abi`), but gyral-rs {} supports {}; \
         see {} for the gyral-rs release that supports each protocol",
        env!("CARGO_PKG_VERSION"),
        Supported,
        PROTOCOLS_URL
    )]
    UnsupportedProtocol {
        /// Protocol declared by the build.
        received: u32,
    },
    /// The manifest's `gyral` is not a stable `MAJOR.MINOR.PATCH` release.
    #[error(
        "the bundle names Gyral {received:?}, which is not a stable MAJOR.MINOR.PATCH release; \
         gyral-rs accepts no prerelease or build metadata, so rebuild with a stable Gyral release"
    )]
    InvalidGyralVersion {
        /// Release text declared by the build.
        received: String,
    },
    /// The build's Gyral release predates its protocol's minimum release.
    #[error(
        "the bundle names Gyral {received}, but SSR protocol {protocol} needs Gyral {minimum} or newer; \
         rebuild the server and browser bundles with a newer Gyral release"
    )]
    GyralTooOld {
        /// Release declared by the build.
        received: GyralVersion,
        /// Oldest release the protocol accepts.
        minimum: GyralVersion,
        /// Protocol declared by the build.
        protocol: SsrProtocol,
    },
    /// The browser URL does not match its digest and canonical asset location.
    #[error("the client path must be /assets/client.<SHA-256>.js")]
    InvalidClientPath,
    /// Style hash count exceeded the bounded document header policy.
    #[error("a manifest can contain at most 32 style hashes")]
    TooManyStyles,
}

/// Lists the supported protocols and their minimum releases for error messages.
struct Supported;

impl fmt::Display for Supported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, protocol) in SUPPORTED_PROTOCOLS.iter().enumerate() {
            let separator = if index == 0 { "" } else { ", " };
            write!(
                f,
                "{separator}protocol {protocol} (Gyral {} or newer)",
                protocol.minimum_gyral()
            )?;
        }
        Ok(())
    }
}

/// A stable Gyral release, `MAJOR.MINOR.PATCH`, ordered numerically.
///
/// Parsing rejects prereleases (`0.3.10-next.0`), build metadata, leading zeros,
/// and any other text, so a release that compares as newer is a real release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GyralVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

impl GyralVersion {
    /// Construct a release from its three numbers.
    #[must_use]
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Major release number.
    #[must_use]
    pub const fn major(self) -> u64 {
        self.major
    }

    /// Minor release number.
    #[must_use]
    pub const fn minor(self) -> u64 {
        self.minor
    }

    /// Patch release number.
    #[must_use]
    pub const fn patch(self) -> u64 {
        self.patch
    }
}

impl TryFrom<&str> for GyralVersion {
    type Error = ManifestError;

    fn try_from(text: &str) -> Result<Self, Self::Error> {
        let invalid = || ManifestError::InvalidGyralVersion {
            received: text.to_owned(),
        };
        let mut parts = text.split('.').map(release_number);
        match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(Some(major)), Some(Some(minor)), Some(Some(patch)), None) => {
                Ok(Self::new(major, minor, patch))
            }
            _ => Err(invalid()),
        }
    }
}

/// One release number: ASCII digits without a sign or a leading zero.
fn release_number(text: &str) -> Option<u64> {
    let canonical = !text.is_empty()
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if canonical { text.parse().ok() } else { None }
}

impl fmt::Display for GyralVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A supported version of the SSR protocol between a Gyral bundle and this renderer.
///
/// The manifest's `abi` field and the bundle's `gyralBundleVersion` both carry it.
/// Only the protocols in [`SUPPORTED_PROTOCOLS`] can be constructed, and each knows
/// the oldest Gyral release that implements it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SsrProtocol {
    version: u32,
    minimum_gyral: GyralVersion,
}

impl SsrProtocol {
    /// Protocol 1: the original bundle interface, implemented by Gyral 0.3.8 and newer.
    pub const V1: Self = Self {
        version: 1,
        minimum_gyral: GyralVersion::new(0, 3, 8),
    };

    /// The protocol number, as written in the manifest's `abi` and `gyralBundleVersion`.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.version
    }

    /// The oldest Gyral release whose bundles this protocol accepts.
    #[must_use]
    pub const fn minimum_gyral(self) -> GyralVersion {
        self.minimum_gyral
    }
}

impl TryFrom<u32> for SsrProtocol {
    type Error = ManifestError;

    fn try_from(received: u32) -> Result<Self, Self::Error> {
        SUPPORTED_PROTOCOLS
            .iter()
            .copied()
            .find(|protocol| protocol.version == received)
            .ok_or(ManifestError::UnsupportedProtocol { received })
    }
}

impl fmt::Display for SsrProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.version)
    }
}

/// A SHA-256 digest. Every 32-byte array is valid; text input is parsed once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Construct from a cryptographic hasher's fixed-size output.
    #[must_use]
    pub const fn from_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// Canonical filename/manifest encoding.
    #[must_use]
    pub fn hex(self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// SRI token and CSP hash source, without the CSP's surrounding single quotes.
    #[must_use]
    pub fn integrity(self) -> String {
        format!("sha256-{}", STANDARD.encode(self.0))
    }
}

impl TryFrom<String> for ContentHash {
    type Error = ManifestError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        if text.len() != 64
            || !text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ManifestError::InvalidHash);
        }
        let mut bytes = [0; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            let pair = text
                .get(index * 2..index * 2 + 2)
                .ok_or(ManifestError::InvalidHash)?;
            *byte = u8::from_str_radix(pair, 16).map_err(|_| ManifestError::InvalidHash)?;
        }
        Ok(Self(bytes))
    }
}

impl From<ContentHash> for String {
    fn from(hash: ContentHash) -> Self {
        hash.hex()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawClient {
    path: String,
    sha256: ContentHash,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    abi: u32,
    gyral: String,
    server_sha256: ContentHash,
    client: RawClient,
    style_hashes: Vec<ContentHash>,
}

/// Validated metadata shared by the server and browser build.
///
/// Decode bounded JSON with `TryFrom<&str>`. All fields are private; the protocol,
/// Gyral release, asset location, digest syntax, and style count are checked together.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawManifest")]
pub struct BundleManifest {
    protocol: SsrProtocol,
    gyral: GyralVersion,
    server: ContentHash,
    client: ContentHash,
    client_path: String,
    styles: Vec<ContentHash>,
}

impl TryFrom<RawManifest> for BundleManifest {
    type Error = ManifestError;

    fn try_from(raw: RawManifest) -> Result<Self, Self::Error> {
        // The minimum release depends on the protocol, so the protocol is checked first.
        let protocol = SsrProtocol::try_from(raw.abi)?;
        let gyral = GyralVersion::try_from(raw.gyral.as_str())?;
        if gyral < protocol.minimum_gyral() {
            return Err(ManifestError::GyralTooOld {
                received: gyral,
                minimum: protocol.minimum_gyral(),
                protocol,
            });
        }
        if raw.client.path != format!("/assets/client.{}.js", raw.client.sha256.hex()) {
            return Err(ManifestError::InvalidClientPath);
        }
        if raw.style_hashes.len() > 32 {
            return Err(ManifestError::TooManyStyles);
        }
        Ok(Self {
            protocol,
            gyral,
            server: raw.server_sha256,
            client: raw.client.sha256,
            client_path: raw.client.path,
            styles: raw.style_hashes,
        })
    }
}

impl TryFrom<&str> for BundleManifest {
    type Error = ManifestError;

    fn try_from(text: &str) -> Result<Self, Self::Error> {
        if text.len() > Self::MAX_BYTES {
            return Err(ManifestError::TooLarge);
        }
        let raw: RawManifest = serde_json::from_str(text).map_err(ManifestError::Json)?;
        Self::try_from(raw)
    }
}

impl BundleManifest {
    /// Maximum metadata size accepted by the bounded JSON entrypoint.
    pub const MAX_BYTES: usize = 16 * 1024;

    /// SSR protocol the bundles implement, from the manifest's `abi`.
    #[must_use]
    pub const fn protocol(&self) -> SsrProtocol {
        self.protocol
    }

    /// Gyral release that built both bundles, from the manifest's `gyral`.
    #[must_use]
    pub const fn gyral_version(&self) -> GyralVersion {
        self.gyral
    }

    /// Expected digest of the compiled server source.
    #[must_use]
    pub const fn server_hash(&self) -> ContentHash {
        self.server
    }

    /// Digest used for the browser filename, SRI, and document CSP.
    #[must_use]
    pub const fn client_hash(&self) -> ContentHash {
        self.client
    }

    /// Canonical same-origin browser entry URL, containing its complete digest.
    #[must_use]
    pub fn client_path(&self) -> &str {
        &self.client_path
    }

    /// Static style digests supplied by the app build; this does not authorize dynamic CSS.
    #[must_use]
    pub fn style_hashes(&self) -> &[ContentHash] {
        &self.styles
    }
}
