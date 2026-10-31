//! Bounded, verified client assets; no server JavaScript artifact or SSR protocol.
use axum::{body::Bytes, http::HeaderValue};
use gyral_renderer_domain::{ContentHash, GyralVersion, SsrProtocol};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use std::{io::Read, path::Path};

#[derive(Debug, thiserror::Error)]
pub(crate) enum AssetError {
    #[error("could not read client artifacts; run pnpm build:client-example")]
    File(#[from] std::io::Error),
    #[error("client artifact exceeds its byte limit or is empty")]
    Size,
    #[error("invalid client manifest")]
    Json(#[from] serde_json::Error),
    #[error("unsupported client manifest format {received}")]
    Format { received: u32 },
    #[error("unsupported Gyral release {received:?}; use a stable Gyral {MINIMUM_GYRAL} or newer")]
    Release { received: String },
    #[error("client artifact digest mismatch")]
    Digest {
        expected: ContentHash,
        actual: ContentHash,
    },
    #[error("invalid document policy header")]
    Policy(#[from] axum::http::header::InvalidHeaderValue),
}

/// The browser-only build has no SSR protocol; it shares gyral-rs's oldest supported release.
const MINIMUM_GYRAL: GyralVersion = SsrProtocol::V1.minimum_gyral();

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    format: u32,
    gyral: String,
    script_sha256: ContentHash,
    css_sha256: ContentHash,
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "RawManifest")]
struct ClientManifest {
    script: ContentHash,
    css: ContentHash,
}

impl TryFrom<RawManifest> for ClientManifest {
    type Error = AssetError;

    fn try_from(raw: RawManifest) -> Result<Self, Self::Error> {
        if raw.format != 1 {
            return Err(AssetError::Format {
                received: raw.format,
            });
        }
        let supported = GyralVersion::try_from(raw.gyral.as_str())
            .is_ok_and(|release| release >= MINIMUM_GYRAL);
        if !supported {
            return Err(AssetError::Release {
                received: raw.gyral,
            });
        }
        Ok(Self {
            script: raw.script_sha256,
            css: raw.css_sha256,
        })
    }
}

#[derive(Debug)]
pub(crate) struct Asset {
    pub(crate) path: String,
    pub(crate) integrity: String,
    pub(crate) bytes: Bytes,
}

impl Asset {
    fn try_new(
        root: &Path,
        hash: ContentHash,
        prefix: &str,
        extension: &str,
        max: u64,
    ) -> Result<Self, AssetError> {
        let filename = format!("{prefix}.{}.{extension}", hash.hex());
        let bytes = read_bounded(&root.join("assets").join(&filename), max)?;
        let actual = ContentHash::from_digest(Sha256::digest(&bytes).into());
        if actual != hash {
            return Err(AssetError::Digest {
                expected: hash,
                actual,
            });
        }
        Ok(Self {
            path: format!("/assets/{filename}"),
            integrity: hash.integrity(),
            bytes: Bytes::from(bytes),
        })
    }
}

#[derive(Debug)]
pub(crate) struct Assets {
    pub(crate) script: Asset,
    pub(crate) css: Asset,
    pub(crate) policy: HeaderValue,
}

impl Assets {
    pub(crate) fn load(root: &Path) -> Result<Self, AssetError> {
        let manifest: ClientManifest =
            serde_json::from_slice(&read_bounded(&root.join("manifest.json"), 4096)?)?;
        let script = Asset::try_new(root, manifest.script, "client", "js", 8 * 1024 * 1024)?;
        let css = Asset::try_new(root, manifest.css, "page", "css", 128 * 1024)?;
        let policy = HeaderValue::try_from(format!(
            "default-src 'none'; script-src '{}'; script-src-attr 'none'; style-src 'self'; style-src-attr 'none'; connect-src 'none'; form-action 'self'; base-uri 'none'; object-src 'none'; frame-ancestors 'none'",
            script.integrity,
        ))?;
        Ok(Self {
            script,
            css,
            policy,
        })
    }
}

fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>, AssetError> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || u64::try_from(bytes.len()).map_err(|_| AssetError::Size)? > max {
        return Err(AssetError::Size);
    }
    Ok(bytes)
}
