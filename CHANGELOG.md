# Changelog

All notable changes to the published gyral-rs crates are recorded here. The four
crates share one version. Before 1.0, an incompatible public API or documented
telemetry-label change increases the minor version; compatible fixes increase
the patch version. The [release runbook](docs/releasing.md) describes how an
entry becomes a crates.io release.

## [0.7.0] - 2026-10-30

### Changed

- **Breaking:** the crates accept bundles built with Gyral **0.3.9** (`GYRAL_VERSION`), and
  reject manifests that name another release, 0.3.8 included. Rebuild the server and browser
  bundles with `@gyral/core` and `@gyral/ssr` 0.3.9, or stay on 0.6.0 for 0.3.8 bundles.
  Gyral 0.3.9 changes mostly development-time checks, testing, and tooling. In the browser,
  `ctx.props` stays the same object while no declared prop's value changes, so code that
  mutates it must copy it first. Server rendering output and the bundle ABI (version 1) are
  unchanged.

## [0.6.0] - 2026-10-29

### Changed

- **Breaking:** the crates accept bundles built with Gyral **0.3.8** (`GYRAL_VERSION`), and
  reject manifests that name another release, 0.3.7 included. Rebuild the server and browser
  bundles with `@gyral/core` and `@gyral/ssr` 0.3.8, or stay on 0.5.0 for 0.3.7 bundles.
  `@gyral/ssr` 0.3.8 takes `@gyral/core` as a peer at its exact version, so list both
  packages, at 0.3.8, in the app that builds the bundles. In the browser, Gyral 0.3.8
  renders a component only when its state, a declared prop, or a declared store's value
  changes; a view that reads anything else stops refreshing on unrelated messages. Server
  rendering output and the bundle ABI (version 1) are unchanged.

## [0.5.0] - 2026-10-28

### Changed

- **Breaking:** the crates accept bundles built with Gyral **0.3.7** (`GYRAL_VERSION`), and
  reject manifests that name another release, 0.3.6 included. Rebuild the server and browser
  bundles with `@gyral/core` and `@gyral/ssr` 0.3.7, or stay on 0.4.0 for 0.3.6 bundles.
  Gyral 0.3.7 starts a component's `init` commands after its first render in the browser,
  replaces a light-DOM component's page children on its first render, and adds template
  rule 16, which rejects a camelCase attribute on a custom element at build time and in
  server rendering; server rendering output for valid templates and the bundle ABI
  (version 1) are unchanged.

## [0.4.0] - 2026-10-14

### Changed

- **Breaking:** the crates accept bundles built with Gyral **0.3.6** (`GYRAL_VERSION`), and
  reject manifests that name another release, 0.3.5 included. Rebuild the server and browser
  bundles with `@gyral/core` and `@gyral/ssr` 0.3.6, or stay on 0.3.0 for 0.3.5 bundles.
  Gyral 0.3.6 adds a development warning for undefined custom-element tags and runs the
  JSX-style tag check in the browser only in development; server rendering and the bundle ABI
  (version 1) are unchanged.

## [0.3.0] - 2026-10-14

### Changed

- **Breaking:** the crates accept bundles built with Gyral **0.3.5** (`GYRAL_VERSION`), and
  reject manifests that name another release, 0.3.4 included. Rebuild the server and browser
  bundles with `@gyral/core` and `@gyral/ssr` 0.3.5, or stay on 0.2.0 for 0.3.4 bundles.
  Gyral 0.3.5 adds a template check for JSX-style component tags; server rendering and the
  bundle ABI (version 1) are unchanged.

## [0.2.0] - 2026-10-13

### Changed

- **Breaking:** the crates accept bundles built with Gyral **0.3.4** (`GYRAL_VERSION`), and
  reject manifests that name another release, 0.3.3 included. Rebuild the server and browser
  bundles with `@gyral/core` and `@gyral/ssr` 0.3.4, or stay on 0.1.0 for 0.3.3 bundles.
  Gyral 0.3.4's server rendering no longer links a module twice and its components scan
  ignores `define()` in comments and strings; the bundle ABI is still version 1.

## [0.1.0] - 2026-10-12

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
