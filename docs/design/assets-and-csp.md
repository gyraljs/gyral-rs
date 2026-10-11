# Assets, CSP, and compatibility

One build supplies the server source, browser entry, and static style hashes.
Rust validates that artifact set at startup. Each HTML response references the
same content-hashed browser URL that the host mounts, with an SRI token derived
from the browser digest. HTML remains private and uncached; verified browser
bytes can be cached for a year as immutable.

## Serving the verified bytes

Retain `AppArtifacts` in application state. Mount `gyral_axum::client_asset` at
`artifacts.manifest().client_path()`. Its body shares the verified startup bytes;
it never rereads a file that may have changed after validation. The response
uses a JavaScript MIME type, `nosniff`, and
`Cache-Control: public, max-age=31536000, immutable`.

The example exposes only this exact asset route. Unknown files, the manifest,
server source, and unsupported methods produce opaque errors with no-store.
Applications that use a filesystem asset service must preserve those boundaries
and avoid attaching immutable caching to misses or arbitrary mutable files.

Rebuild the pair together. Deployments must decide how to retain old hashed
assets for documents already open during a release. This scaffold verifies one
running build; it does not establish a rolling-deployment retention policy.

## Document policy

`DocumentPolicy::try_new(manifest)` builds a restrictive policy for this fixture:
the verified browser entry is allowed by its SHA-256 hash, component and document
styles by their hashes, and connections and form actions at the same origin.
Inline scripts, event handlers, style attributes, plugins, and framing are
denied. The policy uses neither `unsafe-inline` nor `unsafe-eval`.

Use `policy.apply(response)` on successful documents and rendering errors.
The host controls routes and response policy. An app needing images, external
services, extra scripts, dynamic styles, or a different framing policy must
define and test its own explicit CSP. The helper does not infer permissions
from HTML or expand its sources at request time.

The build imports component specs before calling Gyral's `styleHashes()` and
adds the exact document CSS text. This covers the fixture's static styles.
Late registrations and prop-dependent CSS need their own policy design. The
manifest limits the style list to 32 entries to keep generated headers bounded.

CSP hashes depend on the exact style/script bytes. An external script carries
the same hash in its `integrity` attribute; the browser also checks its downloaded
bytes. See the platform's [script policy](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/script-src)
and [style policy](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/style-src)
references. The manifest and hashes require a trusted build; they do not
authenticate an unknown author.

## Tested Gyral behavior

The synthetic example uses Gyral's own Model-View-Intent components, store
instances, provider seeds, `send` commands, `defineForm`, and synchronous Standard
Schema validation. Its view layer is Gyral's, and the build uses JavaScript
modules and esbuild. No Lit or Effect runtime is added to this compatibility
fixture. Rust owns data selection and HTTP behavior.

Two components share one request-seeded visit store. Browser updates reach both
without replacing the server's output nodes. Sequential server requests use
different names and counts; a Gyral render error is followed by a successful
request. A fixed UTC instant verifies New York timezone data on each side of
the daylight-saving transition. Untrusted text is tested in content, form
values, and provider seeds.

The name form works as an ordinary GET without JavaScript. With hydration, Gyral
validates submitted data, preserves rejected text, omits the declared private
field from error state, and updates the browser's shared store. The synthetic
private-note input is disabled until the component receives `Hydrated`, so the
no-JavaScript GET cannot put it in a URL. This example persists no data and
does not test server-side `formAction`, authentication, or a mutation endpoint.

Browser tests delay the client module, type into a server-rendered input, then
verify that hydration preserves both its node and typed value. They also attempt
inline code and styles and substitute a different client response. Chromium
and WebKit must block those attempts while keeping the initial HTML usable.

## Measuring isolate cost

`make measure` runs 50 sequential renders of this actual fixture in release
mode, creating a fresh isolate each time. It reports worker startup, first
render, median, and p95 separately. Timed renders include isolate creation,
bundle evaluation, request encoding, Gyral rendering, and output conversion;
file reads, artifact verification, and HTTP transport are outside them.

These measurements establish a fixture baseline. They do not compare pools or
snapshots, test queue saturation under load, or establish an application's
latency budget. Reusing an isolate would retain globals and stores; snapshots
would need to exclude request data. Keep the fresh-isolate design until a real
app's measurements justify an isolation-preserving optimization. Actual results
are recorded in [verification](../verification.md); follow-up work lives in Beads.
