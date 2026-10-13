use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

/// Rust/JavaScript bundle interface understood by this renderer.
pub const BUNDLE_ABI_VERSION: u32 = 1;
/// Gyral release covered by the current artifact and hydration contract.
pub const GYRAL_VERSION: &str = "0.3.4";

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
    /// The build expects an unsupported renderer interface.
    #[error("unsupported bundle ABI {received}; expected {BUNDLE_ABI_VERSION}")]
    UnsupportedAbi {
        /// Version declared by the build.
        received: u32,
    },
    /// The build uses a Gyral release outside the verified contract.
    #[error("unsupported Gyral release {received}; expected {GYRAL_VERSION}")]
    UnsupportedGyral {
        /// Release declared by the build.
        received: String,
    },
    /// The browser URL does not match its digest and canonical asset location.
    #[error("the client path must be /assets/client.<SHA-256>.js")]
    InvalidClientPath,
    /// Style hash count exceeded the bounded document header policy.
    #[error("a manifest can contain at most 32 style hashes")]
    TooManyStyles,
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
/// Decode bounded JSON with `TryFrom<&str>`. All fields are private; the ABI,
/// release, asset location, digest syntax, and style count are checked together.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawManifest")]
pub struct BundleManifest {
    server: ContentHash,
    client: ContentHash,
    client_path: String,
    styles: Vec<ContentHash>,
}

impl TryFrom<RawManifest> for BundleManifest {
    type Error = ManifestError;

    fn try_from(raw: RawManifest) -> Result<Self, Self::Error> {
        if raw.abi != BUNDLE_ABI_VERSION {
            return Err(ManifestError::UnsupportedAbi { received: raw.abi });
        }
        if raw.gyral != GYRAL_VERSION {
            return Err(ManifestError::UnsupportedGyral {
                received: raw.gyral,
            });
        }
        if raw.client.path != format!("/assets/client.{}.js", raw.client.sha256.hex()) {
            return Err(ManifestError::InvalidClientPath);
        }
        if raw.style_hashes.len() > 32 {
            return Err(ManifestError::TooManyStyles);
        }
        Ok(Self {
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
