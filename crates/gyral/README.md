# gyral

Render [Gyral](https://gyral.dev) components inside a Rust process, then let
Gyral in the browser hydrate the same HTML. This crate is the front door to
[gyral-rs](https://github.com/gyraljs/gyral-rs): one dependency for the embedded
renderer, its validated types, and an optional Axum adapter.

```sh
cargo add gyral serde_json
cargo add gyral --features axum   # also re-export gyral-axum as gyral::axum
```

Your application keeps routes, authorization, data loading, and mutations. It
passes the renderer a validated page name and a JSON object containing only
data the browser may see:

```rust
use gyral::{ContractError, PageId, RenderRequest};
use serde_json::json;

fn main() -> Result<(), ContractError> {
    let page = PageId::try_from("home".to_owned())?;
    let request = RenderRequest::try_new(page, json!({"name": "friend"}))?;
    assert_eq!(request.page().as_str(), "home");
    Ok(())
}
```

At startup, verify the application's build artifacts with `AppArtifacts`, start
a worker with `Renderer::start`, and keep the returned `RendererWorker` for an
explicit shutdown. Each request runs in a fresh V8 isolate with a deadline, a
managed heap budget, and cancellation. With the `axum` feature,
`gyral::axum::render_page` turns a render into a response with
`Cache-Control: no-store`, maps overload to 503, and hides JavaScript
diagnostics behind fixed error bodies.

The renderer accepts trusted, reviewed bundles; it is not a sandbox for
untrusted JavaScript.

## Building blocks

| Crate | Role |
| --- | --- |
| [`gyral-renderer`](https://crates.io/crates/gyral-renderer) | Embedded `deno_core`/V8 worker, artifacts, limits, and telemetry. Re-exported at this crate's root. |
| [`gyral-axum`](https://crates.io/crates/gyral-axum) | Axum responses, verified browser assets, and a content security policy. `gyral::axum` with the `axum` feature. |
| [`gyral-renderer-domain`](https://crates.io/crates/gyral-renderer-domain) | Pure request, manifest, limit, and HTML types without V8. |

Depend on a building block directly when you need less: for example, the domain
crate alone keeps V8 out of a pure application layer. All four crates share one
version and move together.

## Native V8 archive

Building `gyral` links a prebuilt V8 library. Pin and verify that archive in
your application as described in
[the renderer guide](https://github.com/gyraljs/gyral-rs/blob/main/crates/gyral-renderer/README.md#pin-the-native-v8-archive).
The tested target is `x86_64-unknown-linux-gnu` with Rust 1.99 or later.

## Learn more

- [The gyral-rs repository](https://github.com/gyraljs/gyral-rs), including a
  complete [Axum example](https://github.com/gyraljs/gyral-rs/tree/main/examples/axum-app)
- [Architecture](https://github.com/gyraljs/gyral-rs/blob/main/ARCHITECTURE.md) and the
  [security boundary](https://github.com/gyraljs/gyral-rs/blob/main/docs/security.md)

Version 0.1 is pre-1.0: incompatible changes increase the minor version.
Licensed under the [MIT License](https://github.com/gyraljs/gyral-rs/blob/main/LICENSE).
