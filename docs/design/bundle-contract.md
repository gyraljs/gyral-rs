# Bundle contract, ABI 1

Each application supplies one compiled JavaScript bundle. It must define:

```js
globalThis.gyralBundleVersion = 1;
globalThis.gyralRender = async ({ page, props }) => {
  return /* a complete HTML document or fragment */;
};
```

After bundle evaluation, the renderer requires an unsigned integer version equal
to `BUNDLE_ABI_VERSION` before invoking the render entrypoint. Absent, malformed,
and unsupported declarations return `UnsupportedBundleAbi`. Older scaffold bundles
must be rebuilt with the declaration. Version 0.1.0 is the first crates.io release;
this is the first enforced interface, not a promise of compatibility with later ABIs.

`page` is a logical identifier containing 1–64 ASCII letters, digits, hyphens,
or underscores. `props` is a JSON object supplied by the Rust application after
its access checks. The Rust constructor rejects non-object props and serialized
props larger than 256 KiB. The bridge encodes JSON as a string and calls
`JSON.parse`, preserving keys such as `__proto__` without executing values as code.

The result must be a nonempty string no larger than 2 MiB. JavaScript throws,
rejected promises, wrong result types, and invalid output follow named Rust
error variants. HTML is trusted renderer output; the Rust type checks its size
and presence, not its safety. Gyral owns escaping interpolated values.

Server and browser bundles may each be at most 8 MiB. The example uses an IIFE containing its imports,
so the runtime requires no filesystem module loader. Dynamic imports, unresolved
package names, TypeScript compilation at request time, and arbitrary downloads
are outside this contract. Build the browser bundle from the same components
and exact Gyral version as the server bundle. ABI 1's verified artifact manifest
accepts Gyral `0.3.3` and one self-contained browser entry.

The example passes the Gyral SSR result through unchanged. It uses Declarative
Shadow DOM and Gyral's hydration seeds. Do not handcraft those seeds in Rust or
put server authentication secrets into client-visible props.

## Gyral server rendering inside the bundle

Render with `renderToString` from `@gyral/ssr` and return its string. Gyral's
`renderPage` returns a Fetch `Response`; ABI 1 expects a string, and the
embedded runtime provides no Fetch API. The bundle therefore writes its own
document shell, including the `<head>` elements that `renderPage` would manage.

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
avoids a circular build hash while keeping both outputs in one artifact set.

| Manifest field | Meaning |
| --- | --- |
| `abi` | Required unsigned interface version, currently 1. |
| `gyral` | Exact verified framework release, currently `0.3.3`. |
| `server_sha256` | Digest of the UTF-8 bytes in `server.js`. |
| `client.path` | `/assets/client.<client.sha256>.js`, with no alternate path or query. |
| `client.sha256` | Digest of the browser bytes, also used for SRI and CSP. |
| `style_hashes` | At most 32 SHA-256 digests of trusted static style texts. |

Every digest uses 64 lowercase hexadecimal characters. Unknown fields are
rejected at both object levels. `BundleManifest::try_from(json_text)` bounds the
JSON to 16 KiB before decoding and validates versions, paths, and style count.
The Serde implementation also routes through the validated constructor; hosts
using another deserializer must bound input before decoding.

The host reads files at startup, then calls
`AppArtifacts::try_new(manifest, server_source, browser_bytes)`. That constructor
checks source and browser size and verifies both digests. `Renderer::start`
accepts a clone of `artifacts.bundle()`. File reads, route mounting, and trust in
the build's author remain host responsibilities. Digests detect changes against
the supplied manifest; they are not signatures.

The fixture checks actual Gyral stores, shared updates, forms, escaping,
timezone transitions, error recovery, and DOM adoption under CSP. Read
[assets and CSP](assets-and-csp.md) and [verification](../verification.md) for
their exact scope. Streams, split browser chunks, dynamic CSS, and version
upgrades have no compatibility evidence in this interface.
