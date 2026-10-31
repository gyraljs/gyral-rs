# Askama pages with client-only Gyral

The notes app at `/notes` is a multi-page Rust application. Askama renders each
document and its native forms. Axum receives submissions, Rust checks them, and
SQLite stores the result. Gyral adds a live preview after the browser loads the
client entry. Saving and navigation work with JavaScript disabled or blocked.

## Small rendering demonstration

Axum answers `/?name=Mike&start=5` with a complete document rendered by Askama.
The greeting and ordinary form are ready to use. Askama also emits tags with
escaped attributes and readable fallback text:

```html
<gyral-client-greeting name="{{ name }}">…fallback…</gyral-client-greeting>
<gyral-client-counter start="{{ start }}">…fallback…</gyral-client-counter>
```

The browser entry registers these elements through Gyral `define()`, upgrades
the existing tags, and performs their first render. This is client rendering.
Askama renders the surrounding page on the server; Gyral has no prerendered
component contents to hydrate.

## Choose a rendering path

| | Embedded Gyral example | Askama client example |
| --- | --- | --- |
| Executable | `gyral-axum-example` | `gyral-askama-client-example` |
| Page rendering | Gyral through `deno_core` | Askama in Rust |
| Initial component contents | Gyral-rendered HTML and seeds | Tags and fallback text |
| Browser work | Hydrate existing contents | Initialize and render components |
| Server JavaScript engine | Embedded V8 | None |
| Without JavaScript | Prerendered component contents | Askama content and fallbacks |
| Local port | 3037 | 3038 |

Choose Askama with client components when ordinary HTML can carry the essential
experience and widgets can wait for JavaScript. Choose embedded SSR when the
component contents need to arrive in the initial response and adopting that
content matters. Neither example establishes production readiness or an
application performance budget.

## Build and run

After the [development prerequisites](../development.md), run from the repo root:

```sh
make client-demo
make client-browser
make client-docker-smoke
```

The first command builds browser assets and this Rust executable, then serves
<http://127.0.0.1:3038>. The other commands verify the browser path and build/check
the non-root container. `GYRAL_CLIENT_BIND` and `GYRAL_CLIENT_DIST` override the
address and artifact directory; `GYRAL_NOTES_DB` selects the SQLite file. Open
`/notes` for the forms application. Ctrl+C or SIGTERM drains HTTP and closes the
database pool. `/health` reports
HTTP reachability. These commands do not publish or deploy anything.

## Notes lifecycle

| Route | Behavior |
| --- | --- |
| `GET /notes` | List this browser's notes and link to a new note. |
| `GET /notes/new` | Render an empty native form and preview tag. |
| `POST /notes` | Create validated content, then redirect to its detail page. |
| `GET /notes/{id}` | Render the saved note, with edit and delete links. |
| `GET /notes/{id}/edit` | Render a form populated with saved values. |
| `POST /notes/{id}` | Update the owned note, then redirect to its detail page. |
| `GET /notes/{id}/delete` | Ask for deletion confirmation without changing data. |
| `POST /notes/{id}/delete` | Delete the owned note, then redirect to the list. |

Successful POSTs use **303 See Other**. Refreshing the resulting GET does not
repeat the write. There is no client router or fetch-based submission. Concurrent
edits use last-write-wins; this small example has no edit-conflict UI or submission
idempotency key.

Identifiers must be positive integers. Titles contain 1–100 Unicode scalar
values after trimming, with no internal control characters. Bodies contain up to
4,000 scalar values; tabs and newlines are allowed. Native CRLF newlines normalize
to LF. Content is plain text, escaped by Askama and Gyral rather than interpreted
as HTML or Markdown. Rust collects both field errors and returns a 422 form with
submitted values, a linked error summary, `aria-invalid`, and associated messages.
Invalid content never reaches the storage port. Unknown, duplicate, missing, or
malformed fields are rejected by the form extractor; POST bodies are limited to
64 KiB.

### Notebook access and CSRF

The chosen example policy is an anonymous notebook per browser, without account
sign-in. A 256-bit operating-system random capability is stored in an HttpOnly,
SameSite=Strict cookie scoped to `/notes`, with a 30-day lifetime. Every read and
atomic write includes that capability in its ownership condition. A record in
another notebook appears missing; knowing its integer ID grants no access.

Each form carries an HMAC-SHA256 proof bound to its notebook cookie. Rust checks
the proof in constant time before a write; JavaScript is not involved. A POST
without a notebook cookie is forbidden. The signing key is generated at startup.
Restarting preserves stored notes and cookie ownership, while invalidating forms
opened before the restart. An expired proof returns a 403 page with the draft and
a fresh proof, allowing an explicit second submission after review.

The cookie is the access capability: clearing or losing it opens a new notebook,
and this example has no account recovery or cookie rotation. Notes are retained
until deleted. It is a local synthetic-data demonstration over HTTP; the cookie
does not use Secure. A deployed application must supply its own authenticated
ownership, HTTPS cookie policy, recovery, and retention decisions.

### Persistence and workflow boundaries

The default file is `.local/notes.sqlite`, independent of the process working
directory. `GYRAL_NOTES_DB` overrides it. Startup creates its parent directory,
opens a four-connection SQLx 0.9.0 pool, and runs numbered migrations before
binding HTTP. Pool acquisition and SQLite lock waiting have three-second limits.
Reads return at most 200 notes. An immediate write transaction enforces the
200-note quota and ownership through conditional SQL. A full notebook returns a
409 form that preserves the draft. The example does not cap total notebooks or
database size.

The [pure domain](../../examples/notes-domain/src/lib.rs) parses constrained
fields. The [workflows](../../examples/notes-app/src/lib.rs) own transaction
boundaries through storage ports. The [SQLite adapter](../../examples/askama-client-app/src/sqlite.rs)
translates driver failures and decodes rows into validated types. Driver errors,
note contents, cookie capabilities, and CSRF proofs are omitted from request
logs and opaque storage failures. Checked SQL metadata under `.sqlx` contains
query/schema descriptions rather than application records. See
[development](../development.md) for metadata verification and safe local reset.

## Web components and native forms

The editor's `input`, `textarea`, hidden CSRF field, and submit button are native
Askama controls in light DOM. `<gyral-note-preview>` is a sibling of the form.
Its initial attributes contain approved title/body data; a Gyral subscription
driver reads the actual controls on upgrade and observes their `input` events.
This captures text typed before JavaScript loads. The driver returns listener
cleanup to Gyral, so it stops on disconnect and reads current values when
reattached. Its update and view functions remain pure.

The preview renders text and a character count in its shadow root. It never
replaces controls, changes input values, submits, or fetches data. The real form
submits exactly `csrf`, `title`, and `body`, even without a preview.

| Form integration | Native submission behavior |
| --- | --- |
| Native light-DOM controls, with an observing component | Used here. Controls belong to the Askama form and remain usable before JavaScript. Slotted light-DOM controls can preserve this ownership too. |
| A complete native form inside a component's shadow root | Its own form can submit to Rust. Its controls belong to that form; the component must load before they exist. |
| A custom control in an outer form | Requires explicit platform form association and `ElementInternals.setFormValue`, or a deliberate light-DOM bridge. A custom tag alone supplies no submitted value. |

An input inside a shadow root does not join an outer form merely because it has
`name` or `form="note-editor"`. Browser tests insert a temporary shadow input and
verify that its `form` is null and that native `FormData` excludes it. The example
does not claim a Gyral form-associated custom-control API.

## Build and data boundaries

[The browser build](../../tools/build-client-example.mjs) resolves
`gyral-client-only`, the esbuild equivalent of Gyral's
`gyralVitePreset({ clientOnly: true })`. It checks that `hydration-off.js` enters
the build and that the hydration loader and `@gyral/ssr` do not. It bundles the
component source without importing the components into Node or rendering them.
It creates no server JavaScript bundle.

The Rust example uses Axum and Askama directly, plus the domain's pure
`ContentHash` and `GyralVersion` types. Its normal dependency graph excludes `gyral-renderer`,
`gyral-axum`, `deno_core`, and V8. The shared workspace still contains SSR
packages; building the whole workspace also builds those packages.

Client manifest format 1 records the installed Gyral release and SHA-256 digests of
JavaScript and external CSS. It has no SSR protocol, so it accepts the same releases as
gyral-rs's oldest protocol: a stable Gyral 0.3.8 or newer. Startup bounds metadata to
4 KiB, script bytes to 8 MiB, and CSS to 128 KiB; it rejects empty artifacts,
unknown fields, invalid hashes, unsupported format versions, older or prerelease Gyral
releases, and mismatched bytes before HTTP binds. Asset URLs are derived from validated digests. Only the two exact
URLs are served from memory. The manifest and server files are not public.
Hashes require a trusted build and manifest; they do not authenticate a publisher.

Both external assets have SRI. CSP permits the exact script hash, same-origin
stylesheets, and same-origin form submissions. It excludes inline scripts,
style attributes, eval, and component network connections. Gyral uses browser
constructable stylesheets for static component CSS, so no SSR style hashes are
needed. Documents and failures use `no-store`; verified assets use one-year
immutable caching.

Rust bounds query text to 4096 bytes and rejects unknown/duplicate fields.
Names must contain 2–64 Unicode scalar values after trimming, with no internal
control characters; starting counts must be 0–99. Askama autoescapes text,
input values, and quoted attributes. Gyral declares corresponding `prop.string`
and `prop.number` mappings. The example sends no inline bootstrap or JSON.
Complex props would need an explicit data contract; never insert raw JSON into
scripts or attributes.

The ordinary GET form navigates to another Rust-rendered page. Counter changes
stay local, reset on navigation, and stop at 999. Essential content stays outside
the elements, while their fallbacks remain readable if JavaScript is disabled,
blocked, or rejected by integrity checks. The original greeting/counter stays a
small rendering demo alongside the persistent notes flow.

[Dockerfile.client](../../Dockerfile.client) uses Node only to build assets.
A separate Rust target cache builds this executable and checks the dependency
graph. The final image contains the Rust binary and client artifacts, runs as
UID 10001, and handles SIGTERM without a renderer worker. SQLite lives in the
writable `/app/data` volume. A named volume preserves data when the container is
replaced; the container smoke verifies this with the same notebook cookie.

## References

Askama's [Axum integration](https://askama.rs/en/stable/frameworks.html) shows
`Html(template.render()?)` with a typed error. The example pins
[Askama 0.16.1](https://docs.rs/askama/0.16.1/askama/), whose HTML templates
autoescape interpolated values. Registration follows the browser's
[custom-element upgrade behavior](https://developer.mozilla.org/en-US/docs/Web/API/CustomElementRegistry/define).
See esbuild's [custom conditions](https://esbuild.github.io/api/#conditions) and
the pinned Gyral package's `dist/vite.d.ts` and `#hydration-loader` import map.
[Verification](../verification.md) records the commands actually run.

Native POST decoding uses [Axum Form](https://docs.rs/axum/0.8.9/axum/struct.Form.html),
and successful writes use [303 redirects](https://docs.rs/axum/0.8.9/axum/response/struct.Redirect.html).
See [SQLx SQLite options](https://docs.rs/sqlx/0.9.0/sqlx/sqlite/struct.SqliteConnectOptions.html),
[SQLite transactions](https://www.sqlite.org/lang_transaction.html), the HTML
[form-owner rules](https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#association-of-controls-and-forms),
and [form-associated custom controls](https://developer.mozilla.org/en-US/docs/Web/API/ElementInternals/setFormValue).
