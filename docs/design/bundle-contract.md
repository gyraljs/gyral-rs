# Bundle contract: SSR protocol 1

Each application supplies one compiled JavaScript bundle. It must define:

```js
globalThis.gyralBundleVersion = 1;
globalThis.gyralRender = async ({ page, props }) => {
  return /* a complete HTML document or fragment */;
};
```

The number is the bundle's **SSR protocol**, the versioned contract between Gyral's server
code and this renderer. The build manifest repeats it in its `abi` field. gyral-rs accepts a
bundle by the protocol it implements, not by an exact Gyral release: any stable Gyral release
at or above the protocol's minimum is accepted. A Gyral release that keeps the protocol
therefore needs no gyral-rs release. Protocol 1 is the interface that gyral-rs 0.1.0 to
0.7.0 called ABI 1, unchanged; those versions each accepted one exact Gyral release, which
the [changelog](../../CHANGELOG.md) names. Gyral's ADR 0034 records the design.

## Supported protocols

| Protocol | Gyral releases | gyral-rs releases |
| --- | --- | --- |
| 1 | 0.3.8 or newer, stable releases only | 0.8.0 or newer |

`SUPPORTED_PROTOCOLS` holds this table in code, and `SsrProtocol::V1.minimum_gyral()`
returns 0.3.8. `BundleManifest::try_from` checks the manifest before any JavaScript runs:

- An `abi` outside the table fails with `ManifestError::UnsupportedProtocol`, whose message
  names the supported protocols and links to this table.
- A `gyral` that is not a stable `MAJOR.MINOR.PATCH` release fails with
  `InvalidGyralVersion`. Prereleases such as `0.3.10-next.0` and build metadata are rejected
  until Gyral decides how its prereleases are versioned.
- A release below the protocol's minimum, such as 0.3.7, fails with `GyralTooOld`.

There is no upper bound: a Gyral 0.4.0 that keeps protocol 1 is accepted. Gyral 0.3.7
implemented the same interface, but before Gyral's ADR 0033 `@gyral/ssr` could bundle a
second copy of core without warning, so `gyral: "0.3.7"` did not reliably name one release.
Only gyral-rs 0.5.x accepts it. `BundleManifest::protocol()` and `gyral_version()` report a
bundle's values; the SSR example logs both at startup.

## Protocol 1

| Element | Protocol 1 |
| --- | --- |
| Server build | ESM sources bundled into one classic-script IIFE (esbuild `platform: 'neutral'`, `NODE_ENV=production`). Gyral's exports must resolve under those conditions. No module loader, dynamic `import()`, or top-level `await`. |
| Host environment | The ECMAScript built-ins of V8 150 (including `Temporal` and `WebAssembly`), `Intl` with full ICU data, `queueMicrotask`, and `console`. Nothing else: no `TextEncoder`, `btoa`, timers, `fetch` or `Response`, `URL`, `crypto`, DOM, or Node globals. |
| Declaration | `globalThis.gyralBundleVersion`, an unsigned integer naming the protocol, read after evaluation. |
| Entry | `globalThis.gyralRender({ page, props })`. `page` matches `[A-Za-z0-9_-]{1,64}`; `props` is a JSON object of at most 256 KiB, passed through `JSON.parse`. |
| Result | A nonempty string, or a promise of one, of at most 2 MiB: a document or a fragment, passed through unchanged. |
| Errors | A throw or rejection becomes `RenderError::JavaScript`; a non-string, `InvalidResult`; an empty or oversized string, `Contract`; a missing or unsupported declaration, `UnsupportedBundleAbi`. |
| Manifest | `abi`, `gyral`, `server_sha256`, `client.path`, `client.sha256`, and at most 32 `style_hashes`, in at most 16 KiB; unknown fields are rejected. |
| Client and CSP | One self-contained ESM browser entry at `/assets/client.<sha256>.js`, referenced with SRI. Markup needs only what the manifest-derived policy allows: that script by hash, `<style>` texts listed in `style_hashes`, no inline scripts, and no `style` attributes. |
| Bundle sizes | Server and browser bundles of at most 8 MiB each. |

The host environment is part of the protocol because bundles depend on whatever the runtime
provides. The embedded runtime also exposes `deno_core`'s internal `Deno.core` and
`__bootstrap` objects; they are not part of the protocol, and bundles must not use them. A
test renders a probe bundle that checks the promised globals are present and common web and
Node globals absent, so a `deno_core` or V8 upgrade that changes the profile fails `make ci`.

**The HTML is outside the protocol.** Declarative Shadow DOM, hydration seeds, markers, and
islands are a contract between one Gyral release's server and browser code. gyral-rs passes
the rendered string through unchanged and never parses it. Protocol 1 therefore requires both
bundles to come from the same Gyral release, which the manifest's `gyral` field names. Do not
handcraft seeds in Rust or put server authentication secrets into client-visible props.

Errors thrown by Gyral, such as G0078 under `onError: 'throw'`, stay in the diagnostic text
of `RenderError::JavaScript`; gyral-rs does not interpret them.

## What changes the protocol

A protocol **bump** is any change that a host built for protocol N could notice:

- Gyral's server code needs a global outside the host environment, or a newer engine than V8 150.
- Its package exports stop resolving into one IIFE under the server build's conditions.
- `renderToString` changes its result type, its rejection under `onError: 'throw'`, or the
  options embedded entries use (`stores`, `onError`), or `styleHashes()` changes its format.
- Server output needs a resource the manifest-derived policy does not allow: an inline
  script, `style` attributes, a second script or preload, or external CSS.
- The client stops being one self-contained entry, or a manifest field is added, removed, or
  reinterpreted. gyral-rs rejects unknown fields, so even an optional field is a bump.
- gyral-rs tightens a limit or drops a promised global. Loosening a limit is not a bump.

Bug fixes, new APIs, development-time checks, tooling, and changes to the HTML, seeds, or
hydration are not bumps; Gyral 0.3.7, 0.3.8, and 0.3.9 were all in that class. A bump breaks
embedded applications. A gyral-rs minor that supports protocol N+1 ships first; the Gyral
minor that declares N+1 follows it. Supporting a new protocol, dropping one, or raising a
minimum is a gyral-rs minor release. A new Gyral release on a supported protocol needs no
gyral-rs release; raising the minimum is how gyral-rs would shut out a release with a
server-side defect.

## Gyral server rendering inside the bundle

Render with `renderToString` from `@gyral/ssr` and return its string. Gyral's
`renderPage` returns a Fetch `Response`; protocol 1 expects a string, and the
embedded runtime provides no Fetch API. The bundle therefore writes its own
document shell, including the `<head>` elements that `renderPage` would manage.
Gyral's style hasher needs `TextEncoder.encode`, which the host does not provide;
the example bundle defines a small UTF-8 shim.

Since 0.3.1, Gyral isolates component failures during server rendering: a component
whose `init` or view throws renders its `error` view (or nothing), is marked
`data-gyral-error`, and the rest of the page still renders. In that case
`renderToString` resolves, and Rust receives a successful page. Pass
`onError: 'throw'` when any component failure should reject the render instead;
the renderer then returns `RenderError::JavaScript` and the Axum adapter answers
with its opaque 500. The example uses `onError: 'throw'` on every page. A function
`onError` keeps the partial page and lets the bundle record each failure. See
Gyral's [error handling guide](https://gyral.dev/docs/error-handling/#on-the-server).

## Build metadata

The example build produces `server.js`, `manifest.json`, and
`assets/client.<full SHA-256>.js`. The browser is built first; its URL and SRI token
are compiled into the server. The server's digest is computed afterward. This
avoids a circular build hash while keeping both outputs in one artifact set. The
build reads the installed Gyral release for `gyral`, requires `@gyral/core` and
`@gyral/ssr` to be the same release, and takes `abi` from `@gyral/ssr`'s
`SSR_PROTOCOL` export, which Gyral adds in 0.3.10; earlier releases implement protocol 1.

| Manifest field | Meaning |
| --- | --- |
| `abi` | Required SSR protocol, an unsigned integer in the supported table; currently 1. |
| `gyral` | Stable Gyral release that built both bundles, at or above the protocol's minimum. |
| `server_sha256` | Digest of the UTF-8 bytes in `server.js`. |
| `client.path` | `/assets/client.<client.sha256>.js`, with no alternate path or query. |
| `client.sha256` | Digest of the browser bytes, also used for SRI and CSP. |
| `style_hashes` | At most 32 SHA-256 digests of trusted static style texts. |

Every digest uses 64 lowercase hexadecimal characters. Unknown fields are
rejected at both object levels. `BundleManifest::try_from(json_text)` bounds the
JSON to 16 KiB before decoding and validates the protocol, the Gyral release, paths,
and style count. The Serde implementation also routes through the validated constructor;
hosts using another deserializer must bound input before decoding.

The host reads files at startup, then calls
`AppArtifacts::try_new(manifest, server_source, browser_bytes)`. That constructor
checks source and browser size and verifies both digests. `Renderer::start`
accepts a clone of `artifacts.bundle()`. After evaluating the bundle, the renderer
requires its `gyralBundleVersion` to name a supported protocol before invoking the
entry; absent, malformed, and unsupported declarations return `UnsupportedBundleAbi`.
With one supported protocol, that is also the manifest's protocol; a second protocol
will need the renderer to compare the two. File reads, route mounting, and trust in
the build's author remain host responsibilities. Digests detect changes against
the supplied manifest; they are not signatures. The `gyral` field binds nothing: it
is a compatibility statement written by the same build that writes the digests.

## Compatibility evidence

The fixture checks actual Gyral stores, shared updates, forms, escaping,
timezone transitions, error recovery, and DOM adoption under CSP. Read
[assets and CSP](assets-and-csp.md) and [verification](../verification.md) for
their exact scope. The Gyral canary (`make gyral-canary`) builds the example with
protocol 1's minimum and newest releases, 0.3.8 and 0.3.9, and runs that fixture's
renderer test, artifact checks, and Chromium and WebKit suites against each. Releases
between them rest on the protocol holding, not on a tested pair. Streams, split browser
chunks, and dynamic CSS have no compatibility evidence in this protocol.
