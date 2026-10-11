#![allow(clippy::expect_used)]
use super::*;
use axum::{body::to_bytes, http::Method};
use gyral_example_notes_app::{self as workflow, NoteStore as _};
use gyral_example_notes_domain::{NoteContent, NotebookKey};
use tower::ServiceExt as _;

async fn request(
    app: &Router,
    method: Method,
    path: &str,
    cookie: &str,
    body: &str,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_owned()))
        .expect("invariant: test request is valid");
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("invariant: router is infallible");
    let status = response.status();
    let headers = response.headers().clone();
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .expect("invariant: response includes cache_control"),
        "no-store"
    );
    assert!(headers.contains_key(header::CONTENT_SECURITY_POLICY));
    let body = String::from_utf8(
        to_bytes(response.into_body(), 256 * 1024)
            .await
            .expect("invariant: test page is bounded")
            .to_vec(),
    )
    .expect("invariant: response is UTF-8");
    (status, headers, body)
}
fn proof(html: &str) -> &str {
    html.split("name=\"csrf\" value=\"")
        .nth(1)
        .and_then(|value| value.split('"').next())
        .expect("invariant: form includes CSRF field")
}
async fn session(app: &Router) -> (String, String) {
    let (status, headers, html) = request(app, Method::GET, "/notes/new", "", "").await;
    assert_eq!(status, StatusCode::OK);
    let cookie = headers
        .get(header::SET_COOKIE)
        .expect("invariant: response includes set_cookie")
        .to_str()
        .expect("invariant: cookie is ASCII")
        .split(';')
        .next()
        .expect("invariant: generated cookie has a value")
        .to_owned();
    (cookie, proof(&html).to_owned())
}

#[tokio::test]
async fn native_crud_validation_and_ownership_are_enforced_without_javascript() {
    let app = tests::app().await;
    let (cookie, csrf) = session(&app).await;
    let (other_cookie, other_csrf) = session(&app).await;
    let (status, _, html) = request(
        &app,
        Method::POST,
        "/notes",
        &cookie,
        &format!("csrf={csrf}&title=+++&body=%00"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(html.contains("value=\"   \""));
    assert!(html.contains("aria-invalid=\"true\""));
    assert!(html.contains("title-error") && html.contains("body-error"));
    assert!(
        request(&app, Method::GET, "/notes", &cookie, "")
            .await
            .2
            .contains("Your notebook is empty")
    );
    for (sent_cookie, sent_csrf) in [
        ("", csrf.as_str()),
        (cookie.as_str(), ""),
        (cookie.as_str(), other_csrf.as_str()),
        (cookie.as_str(), "bad"),
    ] {
        assert_eq!(
            request(
                &app,
                Method::POST,
                "/notes",
                sent_cookie,
                &format!("csrf={sent_csrf}&title=Test&body=Draft")
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    let (_, _, csrf_html) = request(
        &app,
        Method::POST,
        "/notes",
        &cookie,
        "csrf=expired&title=Preserved&body=Unsaved+text",
    )
    .await;
    assert!(csrf_html.contains("value=\"Preserved\""));
    assert!(csrf_html.contains("Unsaved text"));
    assert_eq!(proof(&csrf_html), csrf);
    let (status, headers, _) = request(
        &app,
        Method::POST,
        "/notes",
        &cookie,
        &format!("csrf={csrf}&title=++First++&body=Line+one%0D%0ALine+two"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = headers
        .get(header::LOCATION)
        .expect("invariant: response includes location")
        .to_str()
        .expect("invariant: redirect is ASCII");
    let html = request(&app, Method::GET, location, &cookie, "").await.2;
    assert!(html.contains("<bdi>First</bdi>"));
    assert!(html.contains("Line one\nLine two"));
    for path in [
        location.to_owned(),
        format!("{location}/edit"),
        format!("{location}/delete"),
    ] {
        assert_eq!(
            request(&app, Method::GET, &path, &other_cookie, "").await.0,
            StatusCode::NOT_FOUND
        );
    }
    for path in [location.to_owned(), format!("{location}/delete")] {
        let fields = if path.ends_with("delete") {
            format!("csrf={other_csrf}")
        } else {
            format!("csrf={other_csrf}&title=Stolen&body=No")
        };
        assert_eq!(
            request(&app, Method::POST, &path, &other_cookie, &fields)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }
    let invalid_body = "x".repeat(4001);
    let (status, _, html) = request(
        &app,
        Method::POST,
        location,
        &cookie,
        &format!("csrf={csrf}&title=Kept+draft&body={invalid_body}"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(html.contains("value=\"Kept draft\""));
    assert!(html.contains(&invalid_body));
    assert!(
        request(&app, Method::GET, location, &cookie, "")
            .await
            .2
            .contains("<bdi>First</bdi>")
    );
    let (status, update_headers, _) = request(
        &app,
        Method::POST,
        location,
        &cookie,
        &format!("csrf={csrf}&title=%3Cscript%3E&body=%3Cimg+src%3Dx%3E"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        update_headers
            .get(header::LOCATION)
            .expect("invariant: response includes location"),
        location
    );
    let html = request(&app, Method::GET, location, &cookie, "").await.2;
    assert!(!html.contains("<bdi><script>"));
    assert!(!html.contains("<img src=x>"));
    assert_eq!(
        request(&app, Method::GET, "/notes/-1", &cookie, "").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &app,
            Method::POST,
            location,
            &cookie,
            &format!("csrf={csrf}&title=One&title=Two&body=X")
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        request(&app, Method::POST, "/notes", "gyral_notes=invalid", "")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &app,
            Method::POST,
            "/notes",
            &format!("{cookie}; {cookie}"),
            ""
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &app,
            Method::POST,
            location,
            &cookie,
            &"x".repeat(65 * 1024)
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let (status, headers, _) = request(
        &app,
        Method::POST,
        &format!("{location}/delete"),
        &cookie,
        &format!("csrf={csrf}"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers
            .get(header::LOCATION)
            .expect("invariant: response includes location"),
        "/notes"
    );
    assert_eq!(
        request(&app, Method::GET, location, &cookie, "").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn real_sqlite_persists_after_reopen_and_rolls_back_abandoned_transactions() {
    let directory = tempfile::tempdir().expect("invariant: test temporary directory is available");
    let path = directory.path().join("notes.sqlite");
    let store = sqlite::SqliteNotes::open(&path)
        .await
        .expect("invariant: fresh database opens");
    let owner = NotebookKey::try_from("a".repeat(64)).expect("invariant: synthetic key is valid");
    let other = NotebookKey::try_from("b".repeat(64)).expect("invariant: synthetic key is valid");
    let content = NoteContent::try_from(("Persistent".to_owned(), "Synthetic text".to_owned()))
        .expect("invariant: synthetic content is valid");
    let id = workflow::create(&store, &owner, &content)
        .await
        .expect("invariant: database is writable");
    {
        let mut tx = store
            .begin()
            .await
            .expect("invariant: database is writable");
        tx.insert(&owner, &content)
            .await
            .expect("invariant: database is writable");
    }
    assert_eq!(
        store
            .list(&owner)
            .await
            .expect("invariant: database is readable")
            .len(),
        1
    );
    store.close().await;
    let reopened = sqlite::SqliteNotes::open(&path)
        .await
        .expect("invariant: migrated database reopens");
    assert_eq!(
        reopened
            .find(&owner, id)
            .await
            .expect("invariant: database is readable")
            .map(|note| note.content),
        Some(content)
    );
    assert!(
        reopened
            .find(&other, id)
            .await
            .expect("invariant: database is readable")
            .is_none()
    );
    assert!(matches!(
        workflow::delete(&reopened, &other, id).await,
        Err(workflow::WriteError::NotFound)
    ));
    assert!(
        reopened
            .find(&owner, id)
            .await
            .expect("invariant: database is readable")
            .is_some()
    );
    reopened.close().await;
}

#[tokio::test]
async fn concurrent_creates_respect_the_notebook_quota() {
    let directory = tempfile::tempdir().expect("invariant: test temporary directory is available");
    let store = sqlite::SqliteNotes::open(&directory.path().join("notes.sqlite"))
        .await
        .expect("invariant: database opens");
    let owner = NotebookKey::try_from("c".repeat(64)).expect("invariant: synthetic key is valid");
    let content = NoteContent::try_from(("Quota".to_owned(), String::new()))
        .expect("invariant: synthetic content is valid");
    for _ in 0..199 {
        workflow::create(&store, &owner, &content)
            .await
            .expect("invariant: quota is not yet full");
    }
    let (a, b) = tokio::join!(
        workflow::create(&store, &owner, &content),
        workflow::create(&store, &owner, &content)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let failure = if a.is_err() { a } else { b };
    assert!(matches!(
        failure,
        Err(workflow::WriteError::Store(workflow::StoreError::Full))
    ));
    assert_eq!(
        store
            .list(&owner)
            .await
            .expect("invariant: database is readable")
            .len(),
        200
    );
    // Exercise the HTTP quota failure using the same full notebook and store.
    let assets = Arc::new(
        Assets::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dist"))
            .expect("invariant: make test builds assets"),
    );
    let app = router(AppState {
        assets,
        notes: Arc::new(store.clone()),
        notebooks: Arc::new(
            notebook::NotebookPolicy::new().expect("invariant: host provides randomness"),
        ),
    });
    let cookie = format!("gyral_notes={}", owner.expose());
    let html = request(&app, Method::GET, "/notes/new", &cookie, "")
        .await
        .2;
    let (status, _, page) = request(
        &app,
        Method::POST,
        "/notes",
        &cookie,
        &format!(
            "csrf={}&title=Retained+title&body=Retained+body",
            proof(&html)
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(page.contains("value=\"Retained title\""));
    assert!(page.contains("Retained body"));
    assert_eq!(
        store
            .list(&owner)
            .await
            .expect("invariant: database is readable")
            .len(),
        200
    );
    store.close().await;
}

#[tokio::test]
async fn corrupt_rows_and_closed_storage_return_opaque_server_errors() {
    use sqlx::Connection as _;
    let directory = tempfile::tempdir().expect("invariant: temporary directory is available");
    let path = directory.path().join("notes.sqlite");
    let store = sqlite::SqliteNotes::open(&path)
        .await
        .expect("invariant: database opens");
    let owner = NotebookKey::try_from("d".repeat(64)).expect("invariant: synthetic key is valid");
    let content = NoteContent::try_from(("Synthetic private title".to_owned(), String::new()))
        .expect("invariant: synthetic content is valid");
    let id = workflow::create(&store, &owner, &content)
        .await
        .expect("invariant: database is writable");
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(&path);
    let mut connection = sqlx::SqliteConnection::connect_with(&options)
        .await
        .expect("invariant: database is accessible");
    // Fault injection bypasses domain constructors: a control character fits DDL length checks.
    sqlx::raw_sql("UPDATE notes SET title = char(10)")
        .execute(&mut connection)
        .await
        .expect("invariant: test corruption fits table constraints");
    connection
        .close()
        .await
        .expect("invariant: test connection closes");
    assert!(matches!(
        store.find(&owner, id).await,
        Err(workflow::StoreError::Corrupt)
    ));
    let assets = Arc::new(
        Assets::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dist"))
            .expect("invariant: make test builds assets"),
    );
    let app = router(AppState {
        assets,
        notes: Arc::new(store.clone()),
        notebooks: Arc::new(
            notebook::NotebookPolicy::new().expect("invariant: host provides randomness"),
        ),
    });
    let cookie = format!("gyral_notes={}", owner.expose());
    for closed in [false, true] {
        if closed {
            store.close().await;
        }
        let (status, _, body) = request(
            &app,
            Method::GET,
            &format!("/notes/{}", id.value()),
            &cookie,
            "",
        )
        .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, "The notebook operation could not be completed.");
    }
}
