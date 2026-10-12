# gyral-renderer

Render [Gyral](https://gyral.dev) components inside a Rust process, then let
Gyral in the browser hydrate the same HTML. The renderer evaluates your
application's compiled server bundle through
[`deno_core`](https://crates.io/crates/deno_core) and V8 on a dedicated thread,
with a bounded queue, deadlines, a managed heap budget, cancellation, and an
explicit shutdown.

Your application keeps routes, authorization, data loading, and mutations. It
passes the renderer a validated page name and a JSON object containing only
data the browser may see. The renderer accepts trusted, reviewed bundles; it is
not a sandbox for untrusted JavaScript.

Most applications depend on the [`gyral`](https://crates.io/crates/gyral) front
door, which re-exports this crate's API at its root. Use `gyral-renderer`
directly when you want the building block alone.

```rust,no_run
use gyral_renderer::{AppArtifacts, BundleManifest, PageId, RenderRequest, Renderer};
use serde_json::json;
use std::{error::Error, fs, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    // Artifacts produced by the application's Gyral build. Bound these reads in
    // production; see `BundleManifest::MAX_BYTES` and `AppBundle::MAX_BYTES`.
    let dist = Path::new("dist");
    let manifest = BundleManifest::try_from(fs::read_to_string(dist.join("manifest.json"))?.as_str())?;
    let server = fs::read_to_string(dist.join("server.js"))?;
    let client_name = format!("client.{}.js", manifest.client_hash().hex());
    let client = fs::read(dist.join("assets").join(client_name))?;
    let artifacts = AppArtifacts::try_new(manifest, server, client)?;

    // Start during process initialization, before other threads use V8.
    let (renderer, worker) = Renderer::start(artifacts.bundle().clone())?;

    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(async {
        let page = PageId::try_from("home".to_owned())?;
        let request = RenderRequest::try_new(page, json!({"name": "friend"}))?;
        let html = renderer.render(request).await?;
        println!("{}", html.as_str());

        // Drain requests, then join the execution and watchdog threads.
        worker.shutdown().await?;
        Ok::<(), Box<dyn Error>>(())
    })
}
```

`Renderer` is a cloneable handle; `RendererWorker` owns the threads. Each request
gets a fresh isolate. Dropping a render future cancels queued work or interrupts
active JavaScript. `Renderer::with_limits` accepts a `RenderLimits` policy; the
defaults are a two-second render deadline, a 64 MiB managed heap budget, and a
two-second shutdown deadline. [`gyral-axum`](https://crates.io/crates/gyral-axum),
also available as `gyral::axum`, maps results to Axum responses.

## Supported builds

The tested target is `x86_64-unknown-linux-gnu` with Rust 1.99 or later.
`gyral-renderer` 0.1 requires `deno_core` exactly 0.412.0, which resolves V8
150.x; the reviewed native archive is V8 **150.4.0**. Other targets may build but
have no support claim.

## Pin the native V8 archive

`deno_core` links a prebuilt static V8 library. Without configuration, the `v8`
crate's build script downloads that archive from the
[rusty_v8 releases](https://github.com/denoland/rusty_v8/releases) for whatever
`v8` version Cargo resolved, and trusts the download. Your project does not
inherit the gyral-rs repository's Cargo configuration, so pin the archive in
your own application:

1. Commit `Cargo.lock` and keep the resolved `v8` version at the reviewed one:

   ```sh
   cargo update -p v8 --precise 150.4.0
   cargo tree -i v8 --edges normal   # must show v8 v150.4.0
   ```

2. Download the matching archive once and verify its size and SHA-256:

   ```sh
   mkdir -p .local/v8
   curl -fL --output .local/v8/librusty_v8_simdutf_release_x86_64-unknown-linux-gnu.a.gz \
     https://github.com/denoland/rusty_v8/releases/download/v150.4.0/librusty_v8_simdutf_release_x86_64-unknown-linux-gnu.a.gz
   echo "f48762ca10d1f1fc605a441c5ae430ec8ce1e9e80f14d78fbc42cb878c30b476  .local/v8/librusty_v8_simdutf_release_x86_64-unknown-linux-gnu.a.gz" \
     | sha256sum --check
   ```

   The file is 38,799,023 bytes. Keep `.local/` out of version control.

3. Point Cargo at the verified file in your project's `.cargo/config.toml`:

   ```toml
   [env]
   RUSTY_V8_ARCHIVE = { value = ".local/v8/librusty_v8_simdutf_release_x86_64-unknown-linux-gnu.a.gz", relative = true, force = true }
   ```

The archive must match the `v8` version in your lockfile. When an upgrade changes
that version, review and verify the new release asset before updating all three
steps. Container builds need the same file inside the build context. The
gyral-rs repository automates this check with
[`tools/prepare-v8.py`](https://github.com/gyraljs/gyral-rs/blob/main/tools/prepare-v8.py)
and [`tools/v8-archive.json`](https://github.com/gyraljs/gyral-rs/blob/main/tools/v8-archive.json).
The pin establishes integrity against the upstream release asset; it is not a
source audit or reproducible build of V8.

## Learn more

- [Architecture](https://github.com/gyraljs/gyral-rs/blob/main/ARCHITECTURE.md) and the
  [bundle contract](https://github.com/gyraljs/gyral-rs/blob/main/docs/design/bundle-contract.md)
- [Runtime limits](https://github.com/gyraljs/gyral-rs/blob/main/docs/design/runtime-limits.md) and the
  [security boundary](https://github.com/gyraljs/gyral-rs/blob/main/docs/security.md)
- [Operations and release](https://github.com/gyraljs/gyral-rs/blob/main/docs/design/operations-and-release.md)
- A complete [Axum example](https://github.com/gyraljs/gyral-rs/tree/main/examples/axum-app)

Version 0.1 is pre-1.0: incompatible changes increase the minor version, and the
gyral-rs crates move together. Licensed under the
[MIT License](https://github.com/gyraljs/gyral-rs/blob/main/LICENSE).
