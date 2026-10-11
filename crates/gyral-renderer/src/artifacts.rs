use crate::{AppBundle, BundleError, BundleManifest, ContentHash};
use sha2::{Digest, Sha256};
use std::{fmt, sync::Arc};

/// Invalid or mixed build artifacts supplied by an application at startup.
#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    /// Server source violated the bundle-size or nonempty-source boundary.
    #[error(transparent)]
    Bundle(#[from] BundleError),
    /// The browser entry is absent or above its startup memory bound.
    #[error("the client bundle must contain 1 byte–8 MiB")]
    ClientSize,
    /// Server bytes do not belong to this manifest.
    #[error("server bundle digest mismatch")]
    ServerDigest {
        /// Expected digest from the manifest.
        expected: ContentHash,
        /// Actual digest of the supplied bytes.
        actual: ContentHash,
    },
    /// Browser bytes do not belong to this manifest.
    #[error("client bundle digest mismatch")]
    ClientDigest {
        /// Expected digest from the manifest.
        expected: ContentHash,
        /// Actual digest of the supplied bytes.
        actual: ContentHash,
    },
}

/// Immutable, verified server/browser pair. The host owns all file reads.
///
/// Digests detect mixed or modified artifacts; they are not signatures or evidence
/// that their author is trusted. Retain these exact client bytes for serving so
/// files changed on disk cannot invalidate the verified pair after startup.
#[derive(Clone)]
pub struct AppArtifacts {
    bundle: AppBundle,
    manifest: BundleManifest,
    client: Arc<[u8]>,
}

impl fmt::Debug for AppArtifacts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppArtifacts")
            .field("manifest", &self.manifest)
            .field("client_bytes", &self.client.len())
            .finish_non_exhaustive()
    }
}

impl AppArtifacts {
    /// Largest browser entry accepted into the immutable startup artifact set.
    pub const MAX_CLIENT_BYTES: usize = 8 * 1024 * 1024;

    /// Verify both byte sequences before executing or serving them.
    ///
    /// # Errors
    /// Returns size errors or a named server/client digest mismatch.
    pub fn try_new(
        manifest: BundleManifest,
        source: String,
        client: Vec<u8>,
    ) -> Result<Self, ArtifactError> {
        let bundle = AppBundle::try_from(source)?;
        if client.is_empty() || client.len() > Self::MAX_CLIENT_BYTES {
            return Err(ArtifactError::ClientSize);
        }
        let server_hash = hash(bundle.0.as_bytes());
        if server_hash != manifest.server_hash() {
            return Err(ArtifactError::ServerDigest {
                expected: manifest.server_hash(),
                actual: server_hash,
            });
        }
        let client_hash = hash(&client);
        if client_hash != manifest.client_hash() {
            return Err(ArtifactError::ClientDigest {
                expected: manifest.client_hash(),
                actual: client_hash,
            });
        }
        Ok(Self {
            bundle,
            manifest,
            client: Arc::from(client),
        })
    }

    /// Verified source handle; cloning shares immutable source, not runtime state.
    #[must_use]
    pub fn bundle(&self) -> &AppBundle {
        &self.bundle
    }

    /// Metadata used by the host's asset route and response policy.
    #[must_use]
    pub fn manifest(&self) -> &BundleManifest {
        &self.manifest
    }

    /// Shared, verified bytes for the browser entry.
    #[must_use]
    pub fn client_bytes(&self) -> Arc<[u8]> {
        Arc::clone(&self.client)
    }
}

fn hash(bytes: &[u8]) -> ContentHash {
    ContentHash::from_digest(Sha256::digest(bytes).into())
}
