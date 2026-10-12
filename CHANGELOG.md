# Changelog

All notable changes to the published gyral-rs crates are recorded here. The four
crates share one version. Before 1.0, an incompatible public API or documented
telemetry-label change increases the minor version; compatible fixes increase
the patch version. The [release runbook](docs/releasing.md) describes how an
entry becomes a crates.io release.

## [0.1.0] - Unreleased

The first crates.io release.

### Added

- `gyral`: the front-door crate. It re-exports the `gyral-renderer` API at its
  root and, with the `axum` feature, the Axum adapter as `gyral::axum`.
- `gyral-renderer`: an embedded `deno_core` 0.412.0 worker that renders a trusted
  Gyral server bundle in a fresh V8 isolate per request. It provides a bounded
  sixteen-position queue, end-to-end render deadlines, a managed heap budget,
  cancellation on drop, explicit shutdown with joined threads, verified startup
  artifacts, readiness status, and category-only telemetry.
- `gyral-axum`: Axum responses with `no-store` and `nosniff` headers, opaque
  error bodies, 503 on overload, immutable verified browser assets, and a
  manifest-derived content security policy.
- `gyral-renderer-domain`: pure, validated page identifiers, render requests,
  limits, bundle manifests, content hashes, and rendered HTML.
- Bundle ABI version 1. Build manifests must declare that ABI and the exact
  Gyral release named by `GYRAL_VERSION`.

### Supported builds

- Tested on `x86_64-unknown-linux-gnu` with Rust 1.99.0, edition 2024.
- V8 150.4.0 through `deno_core` 0.412.0. Applications pin and verify the native
  archive themselves; see the
  [renderer guide](crates/gyral-renderer/README.md#pin-the-native-v8-archive).
- The renderer accepts trusted, reviewed bundles. It is not an untrusted-code
  sandbox.
