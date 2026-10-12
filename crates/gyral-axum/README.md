# gyral-axum

Thin [Axum](https://crates.io/crates/axum) integration for
[`gyral-renderer`](https://crates.io/crates/gyral-renderer). It turns rendered
Gyral HTML into responses with `Cache-Control: no-store` and
`X-Content-Type-Options: nosniff`, maps overload to 503, and returns fixed
error bodies so JavaScript diagnostics never reach the client.

The [`gyral`](https://crates.io/crates/gyral) front door re-exports this crate as
`gyral::axum` with its `axum` feature; most applications use that path.

The adapter does not choose routes, load data, or decide who may see a page.
Handlers do that work, then pass an approved request to the renderer:

```rust,no_run
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use gyral_renderer::{PageId, RenderRequest, Renderer};
use serde_json::json;

async fn home(State(renderer): State<Renderer>) -> Response {
    // Authorize the caller and load data before this point.
    let request = PageId::try_from("home".to_owned())
        .and_then(|page| RenderRequest::try_new(page, json!({"name": "friend"})));
    match request {
        Ok(request) => gyral_axum::render_page(&renderer, request).await.into_response(),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}

fn routes(renderer: Renderer) -> Router {
    Router::new().route("/", get(home)).with_state(renderer)
}
```

`render_page` accepts any `RenderService`, so handler tests can supply a
hand-written fake instead of starting V8. `RenderedPage::with_status` sets a
status chosen by the route, such as a real 404.

For the browser entry, `client_asset` serves verified bytes from `AppArtifacts`
with immutable caching; mount it at `artifacts.manifest().client_path()`.
`DocumentPolicy` builds a content security policy from the same manifest,
allowing that script and the bundle's style hashes without `unsafe-inline`.

Building this crate builds `gyral-renderer` and its V8 dependency. Read
[the renderer's native archive guidance](https://github.com/gyraljs/gyral-rs/blob/main/crates/gyral-renderer/README.md#pin-the-native-v8-archive)
before the first build. The complete
[Axum example](https://github.com/gyraljs/gyral-rs/tree/main/examples/axum-app)
adds readiness, metrics, tracing, and signal shutdown.

Version 0.1 is pre-1.0: incompatible changes increase the minor version, and the
gyral-rs crates move together. The minimum supported Rust version is 1.99.
Licensed under the [MIT License](https://github.com/gyraljs/gyral-rs/blob/main/LICENSE).
