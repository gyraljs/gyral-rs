//! Anonymous notebook cookie and session-bound, server-signed CSRF proofs.
use axum::{
    body::Body,
    extract::State,
    http::{HeaderValue, Method, Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use gyral_example_notes_domain::NotebookKey;
use hmac::{Hmac, Mac as _};
use sha2::Sha256;
use std::sync::Arc;

const COOKIE: &str = "gyral_notes";

#[derive(Debug, thiserror::Error)]
pub(super) enum SessionError {
    #[error("could not obtain operating-system randomness")]
    Random(#[from] getrandom::Error),
    #[error("invalid notebook capability")]
    Invalid(#[from] gyral_example_notes_domain::ParseError),
    #[error("invalid notebook cookie")]
    Cookie,
    #[error("invalid CSRF signing key")]
    Signing,
    #[error("invalid CSRF proof")]
    Proof,
}

pub(super) struct NotebookPolicy([u8; 32]);
impl std::fmt::Debug for NotebookPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotebookPolicy([REDACTED])")
    }
}
#[derive(Clone)]
pub(super) struct NotebookSession {
    pub(super) owner: NotebookKey,
    pub(super) proof: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn random_bytes() -> Result<[u8; 32], SessionError> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes)?;
    Ok(bytes)
}
impl NotebookPolicy {
    pub(super) fn new() -> Result<Self, SessionError> {
        Ok(Self(random_bytes()?))
    }

    fn signer(&self, owner: &NotebookKey) -> Result<Hmac<Sha256>, SessionError> {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.0).map_err(|_| SessionError::Signing)?;
        mac.update(b"gyral-notes-csrf-v1:");
        mac.update(owner.expose().as_bytes());
        Ok(mac)
    }

    pub(super) fn verify(&self, owner: &NotebookKey, proof: &str) -> Result<(), SessionError> {
        if proof.len() != 64 {
            return Err(SessionError::Proof);
        }
        let bytes = proof
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                let text = std::str::from_utf8(pair).map_err(|_| SessionError::Proof)?;
                u8::from_str_radix(text, 16).map_err(|_| SessionError::Proof)
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.signer(owner)?
            .verify_slice(&bytes)
            .map_err(|_| SessionError::Proof)
    }

    fn session(&self, owner: NotebookKey) -> Result<NotebookSession, SessionError> {
        let proof = hex(&self.signer(&owner)?.finalize().into_bytes());
        Ok(NotebookSession { owner, proof })
    }
}

fn cookie(request: &Request<Body>) -> Result<Option<NotebookKey>, SessionError> {
    let mut found = None;
    for header in request.headers().get_all(header::COOKIE) {
        let header = header.to_str().map_err(|_| SessionError::Cookie)?;
        for entry in header.split(';') {
            if let Some((name, value)) = entry.trim().split_once('=')
                && name == COOKIE
            {
                if found.is_some() {
                    return Err(SessionError::Cookie);
                }
                found = Some(NotebookKey::try_from(value.to_owned())?);
            }
        }
    }
    Ok(found)
}

pub(super) async fn session_middleware(
    State(policy): State<Arc<NotebookPolicy>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let key = match cookie(&request) {
        Ok(key) => key,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                "Invalid notebook cookie. Clear this site's cookie and reload.",
            )
                .into_response();
        }
    };
    let fresh = key.is_none();
    if fresh && request.method() != Method::GET && request.method() != Method::HEAD {
        return (
            StatusCode::FORBIDDEN,
            "Open the notebook first, then submit its form.",
        )
            .into_response();
    }
    let session = (|| {
        let owner = match key {
            Some(owner) => owner,
            None => NotebookKey::try_from(hex(&random_bytes()?))?,
        };
        policy.session(owner)
    })();
    let session = match session {
        Ok(session) => session,
        Err(_) => {
            tracing::error!(
                category = "notebook_session",
                "could not establish notebook session"
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "The notebook could not be opened.",
            )
                .into_response();
        }
    };
    let set_cookie = format!(
        "{COOKIE}={}; Path=/notes; HttpOnly; SameSite=Strict; Max-Age=2592000",
        session.owner.expose()
    );
    request.extensions_mut().insert(session);
    let mut response = next.run(request).await;
    if fresh && let Ok(value) = HeaderValue::from_str(&set_cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}
