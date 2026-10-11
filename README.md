# gyral-rs

Render Gyral components inside a Rust process, then hydrate them with Gyral in
the browser. The embedded renderer uses `deno_core`; Node is needed to build the
example's JavaScript bundles, not to run its server.

The workspace has two application-facing libraries:

| Package | Purpose |
| --- | --- |
| `gyral-renderer` | Execute a trusted app bundle using permission-filtered JSON data and return HTML. |
| `gyral-axum` | Turn rendering results into Axum responses with private caching and opaque errors. |

An internal `gyral-renderer-domain` crate holds validated request and HTML types.
Its separate dependency graph keeps runtime and HTTP code out of the pure core.
All packages retain `publish = false`. Consumers use Cargo path dependencies from
a reviewed source checkout; see the [distribution policy](docs/design/operations-and-release.md).

## Run embedded SSR

Install the tools in [development](docs/development.md), then run from this directory:

```sh
pnpm install --frozen-lockfile
pnpm build:example
make prepare-native
cargo run --locked -p gyral-axum-example
```

Open <http://127.0.0.1:3037/?name=Mike>. Rust supplies the greeting data; Gyral
renders the greeting, counter, seeded shared store, name form, and local time
before the response is sent. The browser adopts that HTML and activates the
controls under an enforcing CSP. The example contains synthetic data only.

## Run Askama pages with browser components

For a Rust server that executes no JavaScript, run:

```sh
pnpm install --frozen-lockfile
make client-demo
```

Open <http://127.0.0.1:3038/notes>. Create, read, edit, and delete synthetic notes
through ordinary pages and HTML forms. Rust validates submissions, checks notebook
ownership and CSRF tokens, and saves changes in SQLite. Successful saves redirect
to the saved note; invalid forms keep the draft and explain the errors.

Gyral adds a live preview in the browser. The form controls remain in Askama's
HTML, so saving and navigation work with JavaScript disabled or blocked.
The notebook cookie identifies this browser's notes. Keep that cookie to return
to them; clearing it opens a new notebook. The database defaults to
`.local/notes.sqlite` and survives server restarts.

The original greeting/counter demonstration remains at
<http://127.0.0.1:3038/?name=Mike&start=5>.

This executable uses Axum and Askama directly. Its dependency graph includes the
pure hash type, but excludes `gyral-renderer`, `gyral-axum`, `deno_core`, and V8.
Node builds its browser assets; the running Rust server needs no JavaScript engine.
Read the [client-only example](docs/design/askama-client.md) for the boundary,
tradeoffs, and Docker commands.

```sh
make check
make test
make browser
make ci
```

The renderer enforces configurable deadlines, a managed V8 heap budget, future
cancellation, and bounded shutdown. See [runtime limits](docs/design/runtime-limits.md)
for the defaults and failure behavior. It remains an executable scaffold for
trusted application code. The verified manifest supplies content-hashed browser
assets, SRI, and static style hashes; see [assets and CSP](docs/design/assets-and-csp.md).
The example supplies renderer readiness, request IDs, redacted telemetry, signal
shutdown, and local release evidence. See [operations and release](docs/design/operations-and-release.md)
for the supported Linux target, fixture load budgets, and distribution policy.
The [runtime evaluation](docs/design/runtime-evaluation.md) compares test-only
snapshot startup and worker concurrency. Production startup still evaluates the
bundle in a fresh isolate for each request. Compatibility with complete
applications remains follow-up work in Beads. Read [security](docs/security.md) and
[verification](docs/verification.md) before using it in a deployed service.

Start with [AGENTS.md](AGENTS.md), [architecture](ARCHITECTURE.md), and the
[bundle contract](docs/design/bundle-contract.md).
