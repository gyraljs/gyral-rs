# Verification evidence

## Gyral 0.3.6 update — October 14, 2026

Gyral 0.3.6 adds a development warning (G0014) for custom-element tags that stay undefined
after load, runs template rule 15 in the browser only in development, and removes the empty
`@gyral/ssr/hydrate` entry point, which the examples never imported. Server rendering and the
bundle ABI are unchanged. This update pins npm `@gyral/core` and `@gyral/ssr` to `0.3.6`,
which npm serves with provenance, and moves the pnpm release-age exemption from the two 0.3.5
versions to these two. As before, the old exemption was kept only while the lockfile was
regenerated, then removed; the frozen installation passes the age gate with the 0.3.6
exemption alone. The regenerated lockfile changes only the two Gyral entries, and their
SHA-512 integrity matches the registry. The domain manifest's `GYRAL_VERSION`, both example
builds, the policy check, and the release tools now require 0.3.6. The published 0.3.0 crates
still accept only 0.3.5 manifests; the new `GYRAL_VERSION` reaches crates.io in 0.4.0 (a
minor release, because 0.3.5 manifests are rejected).

`make ci` passed: **65 nextest tests** (one skipped, the explicit runtime measurement),
doctests, strict Clippy, formatting, Rust docs, frozen npm installation, policy over 29
documents and four publishable crates, artifact, client-artifact and operating checks, the
release-tool regressions, all Chromium/WebKit SSR, client and notes suites, the notes schema
check, `cargo deny`, and `cargo audit`. `cargo deny` and `cargo audit` first failed on four
rustls 0.23.45 advisories (RUSTSEC-2026-0346 to 0349) published the same day; rustls is reached
only by the unpublished axum example's OTLP exporter, and the fixed 0.23.46 is younger than the
two-week dependency rule, so both gates ignore the four IDs until `gyral-rs-4db` upgrades it
(see [security](security.md)). The Askama client entry grows from 78,391 to **79,169 bytes**
(external CSS unchanged at 5,267 bytes). esbuild inlines Gyral's `DEV = false` only after tree
shaking, so the development-only undefined-tag watcher and rule 15's element list stay in the
bundle as code that never runs; Vite and Rollup leave them out. Gyral tracks this as
`gyral-gpr`. Container images, release evidence, packaging, and the runtime evaluation were not
rerun.

## Gyral 0.3.5 update — October 14, 2026

Gyral 0.3.5 adds template rule 15 to `@gyral/core`: a start tag that begins with a capital
letter, has no hyphen and is not an HTML element (a JSX-style `<SignupForm>`) is a template
error. `@gyral/ssr` has no changes of its own. This update pins npm `@gyral/core` and
`@gyral/ssr` to `0.3.5`, which npm serves with provenance, and moves the pnpm release-age
exemption from the two 0.3.4 versions to these two. As for 0.3.4, the old exemption was kept
only while the lockfile was regenerated, then removed; the frozen installation passes the age
gate with the 0.3.5 exemption alone. The regenerated lockfile changes only the two Gyral
entries, and their SHA-512 integrity matches the registry. The domain manifest's
`GYRAL_VERSION`, both example builds, the policy check, and the release tools now require
0.3.5. The published 0.2.0 crates still accept only 0.3.4 manifests; the new `GYRAL_VERSION`
reaches crates.io in 0.3.0 (a minor release, because 0.3.4 manifests are rejected).

`make ci` passed: **65 nextest tests** (one skipped, the explicit runtime measurement),
doctests, strict Clippy, formatting, Rust docs, frozen npm installation, policy over 29
documents and four publishable crates, artifact, client-artifact and operating checks, the
release-tool regressions, all Chromium/WebKit SSR, client and notes suites, the notes schema
check, `cargo deny`, and `cargo audit`. The Askama client entry grows from 77,003 to **78,391
bytes** (external CSS unchanged at 5,267 bytes): it is bundled with esbuild, without
`@gyral/core`'s Vite preset, so it keeps the runtime template checker, which now carries rule
15's list of HTML element names. Container images, release evidence, packaging, and the
runtime evaluation were not rerun.

## Gyral 0.3.4 update — October 13, 2026

Gyral 0.3.4 changes `@gyral/ssr`, whose `renderPage({ components })` tail no longer repeats
links the page head already wrote, and the build-time component scan in `@gyral/core`'s
Vite preset, which now skips `define()` calls inside comments and literals. This update
pins npm `@gyral/core` and `@gyral/ssr` to `0.3.4`, which npm serves with provenance, and
moves the pnpm release-age exemption from the two 0.3.3 versions to these two. The 0.3.3
exemption was kept only while the lockfile was regenerated, then removed; the frozen
installation passes the age gate with the 0.3.4 exemption alone. The regenerated lockfile
changes only the two Gyral entries, and their SHA-512 integrity matches the registry. The
domain manifest's `GYRAL_VERSION`, both example builds, the policy check, and the release
tools now require 0.3.4. The published 0.1.0 crates still accept only 0.3.3 manifests;
the new `GYRAL_VERSION` reaches crates.io in 0.2.0 (a minor release, because 0.3.3 manifests are
rejected).

`make ci` passed: **65 nextest tests**, **ten doctests**, strict Clippy, formatting,
Rust docs, frozen npm installation, policy over 29 documents and four publishable crates,
artifact, client-artifact and operating checks, five release-tool regressions, all
Chromium/WebKit SSR, client and notes suites, the notes schema check, `cargo deny`, and
`cargo audit`. The explicit runtime-measurement test was skipped as usual. The Askama
client entry is still 77,003 bytes, with 5,267 bytes of external CSS. Container images,
release evidence, packaging, and the runtime evaluation were not rerun.

## crates.io release preparation — October 12, 2026

The four library crates, `gyral-renderer-domain`, `gyral-renderer`, `gyral-axum`,
and the new `gyral` front door, now carry registry metadata, READMEs whose Rust
examples compile as doctests, a license copy, and an `include` list. Examples set
`publish = false`. Library requirements became caret ranges, except `deno_core`
`=0.412.0`; the [release runbook](releasing.md) records why. The renderer no longer
asks Tokio for `net` and `signal`; the two example executables request them.
The SSR example now depends on `gyral` with its `axum` feature. Nothing was
published, and no crates.io token or GitHub setting was created or changed.

`make ci` passed with Rust 1.99.0 on Linux x86_64: **65 nextest tests** with the
usual runtime-measurement test skipped, **ten doctests** including every crate
README, strict Clippy, formatting, Rust docs with all features, policy, artifact,
client-artifact and operating checks, four release-tool regressions, all
Chromium/WebKit SSR, client and notes suites, the notes schema check, `cargo deny`
with duplicate-version warnings only, and `cargo audit` with the existing scoped
exception.

`make package` passed. `cargo publish --dry-run --workspace --locked` packaged and
built each crate from its extracted contents in dependency order, then skipped the
upload. The packages contain only `Cargo.toml`, `Cargo.lock`, the README, the
license, and `src/**/*.rs`:

| Crate | Files | Compressed |
| --- | --- | --- |
| `gyral-renderer-domain` | 9 | 10.6 KiB |
| `gyral-renderer` | 13 | 37.0 KiB |
| `gyral-axum` | 8 | 20.3 KiB |
| `gyral` | 7 | 18.7 KiB |

The docs.rs simulation extracted the four packages outside the checkout, so the
repository's forced `RUSTY_V8_ARCHIVE` configuration did not apply. With
`DOCS_RS=1`, no `RUSTY_V8_*` variables, Cargo offline, and an unreachable HTTP
proxy, `cargo doc --all-features` documented every crate with warnings denied.
The V8 build script used its bundled bindings and downloaded nothing. A clean
target directory reproduced the result.

A minimal-version check used stable Cargo 1.99 with `RUSTC_BOOTSTRAP=1` in a
scratch copy. `-Z direct-minimal-versions` resolved every direct dependency to its
lower bound, which equals the verified version; the four libraries then checked
with all features, and the domain tests passed. Full `-Z minimal-versions` failed
to compile the upstream transitive crate `wyz` against the oldest `tap` its manifest
allows; no gyral-rs requirement was involved.

cargo-semver-checks 0.51.0 ran with Rust 1.99.0 against `origin/main` as a Git
baseline for the three existing crates: 202 checks passed for each. The release
workflow runs it against crates.io baselines and skips crates with none.
actionlint 1.7.7 reported no findings in either workflow. Each action is pinned to
the commit that its tag resolves to through the GitHub API: checkout 7.0.1,
setup-node 7.1.0, cache 6.1.0, upload-artifact 7.0.2, download-artifact 8.0.1,
and `rust-lang/crates-io-auth-action` 1.0.5. The release workflow has not run on
GitHub.

The first CI push run of this branch failed in `make release-evidence`: the source
archive omitted `CHANGELOG.md`, so `tools/check.py` inside the extracted archive
reported it missing while `AGENTS.md` and `README.md` linked to it. The archive now
includes it, and a release-tool regression asserts that every repository document
the policy check requires is in the source list. The owner has since published a
`gyral` 0.0.1 name-holding placeholder; `tools/release.py semver` skips a crate whose
published versions are all below 0.1.0, and a second new regression covers that.

After rebasing onto the Gyral 0.3.3 update, `make ci` passed again: **65 nextest
tests**, **ten doctests**, policy over 29 documents and four publishable crates,
**five release-tool regressions**, all Chromium/WebKit suites, `cargo deny`, and
`cargo audit`. `make package` produced the same four packages and sizes as above,
and `make release-evidence` passed: container smoke, evidence build, and the
release smoke, including the policy check in the extracted source archive.

## Gyral 0.3.3 update — October 12, 2026

Gyral 0.3.3 changes only `@gyral/mcp` (one `gyral-mcp` bin); the core and SSR code that
the examples bundle is unchanged from 0.3.2. This update pins npm `@gyral/core` and
`@gyral/ssr` to `0.3.3`, which npm serves with provenance, and moves the pnpm release-age
exemption from the two 0.3.2 versions to these two. The regenerated lockfile changes only
the two Gyral entries, and their SHA-512 integrity matches the registry. The domain
manifest's `GYRAL_VERSION`, both example builds, the policy check, and the release tools
now require 0.3.3.

`make ci` passed: **65 nextest tests**, **four doctests**, strict Clippy, formatting,
Rust docs, frozen npm installation, policy, artifact, client-artifact and operating
checks, four release-tool regressions, all Chromium/WebKit SSR, client and notes
suites, the notes schema check, `cargo deny`, and `cargo audit`. The explicit
runtime-measurement test was skipped as usual. The Askama client entry is still 77,003
bytes, with 5,267 bytes of external CSS. Container images, release evidence, and the
runtime evaluation were not rerun; the figures recorded below for them describe 0.3.1.

## Gyral 0.3.2 update — October 12, 2026

This update pins npm `@gyral/core` and `@gyral/ssr` to `0.3.2`, which npm serves with
provenance, and moves the pnpm release-age exemption from the two 0.3.1 versions to
these two. The regenerated lockfile changes only the two Gyral entries, and their
SHA-512 integrity matches the registry. The domain manifest's `GYRAL_VERSION`, both
example builds, the policy check, and the release tools now require 0.3.2.

`make ci` passed: **65 nextest tests**, **four doctests**, strict Clippy, formatting,
Rust docs, frozen npm installation, policy, artifact, client-artifact and operating
checks, four release-tool regressions, all Chromium/WebKit SSR, client and notes
suites, the notes schema check, `cargo deny`, and `cargo audit`. The explicit
runtime-measurement test was skipped as usual. The Askama client entry is now 77,003
bytes, with 5,267 bytes of external CSS. Container images, release evidence, and the
runtime evaluation were not rerun; the figures recorded below for them describe 0.3.1.

## Dependency refresh — October 11, 2026

A registry review updated all eight outdated direct Rust dependencies and both
JavaScript build/test dependencies. The following pins were verified on Linux
x86_64 with Rust 1.99.0:

| Set | Verified versions |
| --- | --- |
| Rust data and errors | Serde 1.0.229, serde_json 1.0.154, thiserror 2.0.21 |
| Runtime and properties | Tokio 1.53.2, proptest 1.12.0 |
| Encoding and cryptography | base64 0.23.1, SHA-2 0.11.0, HMAC 0.13.0 |
| JavaScript tools | esbuild 0.28.2, Playwright 1.64.0, pnpm 12.11.2, Node 24.21.0 |
| Verification tools | cargo-nextest 0.9.148, SQLx CLI 0.9.0 |
| CI actions | checkout 7.0.1, setup-node 7.1.0, cache 6.1.0, upload-artifact 7.0.2; each pinned to its full commit hash |

Gyral 0.3.1, deno_core 0.412.0, Axum 0.8.9, and SQLx 0.9.0 were already current
in the reviewed registries. The resolved V8 150.4.0 archive retains its reviewed
digest. Cargo refreshed compatible transitive versions; older versions required
by upstream consumers remain, including SQLx's SHA-2 0.10.9.

`make ci` passed: **65 nextest tests**, **four doctests**, strict Clippy, formatting,
Rust docs, frozen npm installation, architecture/skill/link policy, artifact and
operating checks, both example builds, all Chromium/WebKit suites, four release-tool
regressions, SQL metadata, and dependency policy. One explicit runtime-measurement
test was skipped. SQLx CLI 0.9.0 also passed its empty-database preparation check
separately. The new fixed-vector test confirms that the combined SHA-2/HMAC upgrade
preserves the CSRF protocol bytes, using a value computed independently with
Python's standard-library HMAC.

The domain mutation run examined **89 mutations**: **75 caught**, **14 unable to
compile**, and **none survived**. The self-review found no new domain dependency,
untyped error, unchecked cast, panic path, or unbounded operation.

Both production images passed their smoke checks. The Askama image retained native
CRUD, SQLite persistence and cookie ownership across container replacement; its
dependency graph excludes V8. The SSR image passed verified assets, CSP, readiness,
metrics, opaque errors, and joined signal shutdown. `make release-evidence` passed
archive checksums, connected inventories, native integrity, license texts, source
policy, and extracted-binary checks. Source artifacts include the pnpm configuration.

`make load` passed the existing synthetic budgets: **2,080 successful requests**
over 60.07 seconds, **34.63 requests/second**, **175.56 ms p95**, **208.22 ms p99**,
**40.35 MiB peak RSS**, and **0.65 MiB RSS growth**. Other build and background
activity was present. This is regression evidence for the fixture, not an app SLO.

No RustSec exception was added. The existing compile-time `paste` maintenance
exception remains because the current embedded runtime still requires it.
Logs remain under ignored `.local/dependency-*.log` paths. These checks did not
publish crates, deploy an application, or change a development database.

The [CI workflow](../.github/workflows/ci.yml) then passed on GitHub's hosted
`ubuntu-24.04` runner for both the pull request and the merged commit `85dc860`
(run 38177598445, 24 minutes 42 seconds). That run executed `make ci`,
`make client-docker-smoke`, `make release-evidence`, and `make load`, and
uploaded the evidence archive as a workflow artifact. It published nothing.

A review on October 12, 2026 repeated `make check`, `make test`, `cargo deny check`,
and strict Rust docs locally: 65 nextest tests passed with one skipped, four doctests
passed, and cargo-deny reported only duplicate-version warnings. The refresh adds
duplicates that upstream crates still require: `deno_core` uses base64 0.22, and
SQLx uses SHA-2 0.10 with its digest stack. Cargo also resolved `crypto-common`
0.1.6 rather than 0.1.7 because 0.1.7 requires exactly `generic-array` 0.14.7.

## Earlier scaffold evidence

The earlier evidence below was collected locally on October 11, 2026, on Linux x86_64. It used vendored
Gyral `0.3.1-next.9`, `deno_core` 0.412.0, and resolved V8 150.4.0. Tools were
Rust 1.99.0, Node 24.15.0, pnpm 10.33.2, cargo-nextest 0.9.143,
cargo-deny 0.20.2, cargo-mutants 27.1.0, SQLx CLI 0.8.3, and Playwright 1.63.0.

| Check | Observed result |
| --- | --- |
| `make ci` | Passed frozen JavaScript install, skill/vendor/link policy, formatting, strict Clippy, native build, Rust docs, artifact startup checks, browser smoke, domain-only build, and dependency checks. |
| Rust nextest | All 56 tests passed in isolated processes, including ten Askama example tests and seven notes domain/workflow tests. Twelve property tests each use 128 cases and a fixed seed. |
| Rust documentation tests | All four examples passed; the renderer example is compiled without execution. |
| Embedded runtime | Promise rejection, non-string results, empty/oversized HTML, Unicode and injection-shaped JSON, preserved `__proto__` data, repeated-request isolation, worker shutdown, and New York timezone formatting passed. Tested `fetch` and `Deno.readTextFile` were absent. |
| Resource interruption | Bundle initialization, synchronous infinite loops, and infinite microtask chains returned typed deadline failures. Managed-heap pressure returned the configured heap-budget error. After recoverable failures, another request rendered successfully on the same worker. |
| Cancellation and lifecycle | Dropping an active render interrupted JavaScript. Owner drop, shutdown cancellation, and shutdown expiry closed admission and interrupted active work. A full queue returned `Busy`; queued work had its own deadline. A simulated upstream Rust unwind disposed its isolate and allowed another request. |
| Host configuration | Rendering without a Tokio runtime or enabled timer driver returned `HostRuntimeUnavailable` before admitting work. Shutdown in an invalid host context returned its typed error and still ended both native threads through the Drop fallback. |
| Output allocation | A compact 500 MiB JavaScript rope returned `HtmlTooLarge` before copying into Rust. On Linux, that test's peak resident memory stayed below its 300 MiB regression threshold. This is a single output-path check, not a general process-memory guarantee. |
| Bundle ABI and artifacts | Missing, malformed, and unsupported JavaScript ABI declarations returned `UnsupportedBundleAbi` before invoking app code. Manifest versions, unknown fields, digests, canonical paths, byte limits, and style-count limits passed. Changed server/client bytes were rejected. |
| Build/startup smoke | Unchanged inputs reproduced the same manifest. Five altered or missing build sets exited with an error before HTTP startup: server digest, browser digest, manifest ABI, oversized metadata, and missing metadata. |
| Actual Gyral bundle | Escaping, Declarative Shadow DOM, component/provider seeds, manifest URL/SRI, and emitted style hashes passed. Sequential request stores retained their own names/counts. The New York fall-back transition rendered 1:30 AM in EDT and EST. A Gyral view error was followed by a healthy render. |
| Axum adapter | Application status, HTML MIME, no-store and nosniff policy, opaque failures, resource/ABI error mapping, and a hand-written rendering capability passed. Verified asset bytes had immutable caching and JavaScript MIME; the bounded CSP covered static styles and opaque errors. |
| Chromium and WebKit | Both passed no-JavaScript form navigation, seeded stores, shared updates, invalid-form text/ARIA/redaction, timezone display, escaping, keyboard controls, both themes, 320-pixel layout, and 200% text scaling. Hydration retained counter/store outputs, the form control and time node, including text typed before the client loaded. Consoles and CSP records stayed clean during supported interactions. |
| CSP/SRI enforcement | In both browsers, attempted inline JavaScript and style attributes were blocked. Substituted client bytes did not execute or register components; SSR stayed visible. No `unsafe-inline` or `unsafe-eval` was permitted. |
| Domain mutation check | 89 mutations examined: 75 caught and 14 unable to compile; none survived. Coverage includes request/output/resource boundaries and the manifest/hash model. |
| `make docker-smoke` | Linux amd64 release image built. Its container served uncached SSR, matching hashed client bytes/SRI/CSP, immutable asset caching, uncached misses, and opaque errors. It ran as UID 10001, had no Node executable, and exited with code zero on Docker's configured SIGINT stop. |
| `make measure` | Release-mode run of 50 sequential real-fixture renders with a fresh isolate each time; results and scope below. |

The tested container image ID was
`sha256:e111b674091928870e0410108d6af7554bca97afde276b6b436c99692e55e2ad`.
Local reports are retained in the ignored `.local` directory. Commands and their
purpose are documented in [development](development.md).

## Askama client example

This evidence also predates the npm migration described under
[published Gyral](#published-gyral-and-runtime-evaluation); the example now
uses the published 0.3.4 release.

The separate `gyral-askama-client-example` pins Askama 0.16.1 and the same
vendored Gyral release. Its browser entry is 74,926 bytes and its external page
CSS is 5,267 bytes. It builds no server JavaScript artifact.

| Check | Observed result |
| --- | --- |
| `make ci` | Passed the complete workspace gate with both examples, 56 nextest tests, four doctests, strict Clippy/docs, startup/browser checks, and dependency policy. |
| Client artifact smoke | Normal Cargo graph excluded `gyral-renderer`, `gyral-axum`, `deno_core`, and V8. Unchanged builds reproduced metadata. Ten invalid artifact sets failed before HTTP bind: altered JS/CSS, unsupported format/release, malformed hash, unknown metadata field, oversized metadata/CSS, empty CSS, and missing script. Two invalid database paths also failed before bind. |
| Askama HTTP contract | Response contained custom-element tags and fallback text, without Gyral shadow content, hydration seeds, or counter output. Invalid/unknown/duplicate query parameters returned uncached 400 responses. Template diagnostics stayed out of the opaque 500 body. |
| Chromium and WebKit | Both verified no-JavaScript form/navigation, visible fallbacks while the entry was delayed, first client rendering from string/number attributes, preserved text typed into the ordinary form before client rendering, counter increment/reset, escaping in Askama and Gyral, keyboard focus, both themes, 320-pixel layout, and 200% text scaling. Supported interactions produced no console errors or CSP violations. |
| Client CSP/SRI | Both browsers blocked attempted inline script/style attributes. Blocked or substituted JavaScript left readable fallbacks and working navigation; substituted bytes did not register elements or execute. |
| Notes Rust contracts | Native CRUD/303, accumulated 422 field errors, preserved drafts on expired CSRF proofs and full notebooks, ownership-scoped reads/writes, malformed/duplicate form fields, bad IDs/cookies, oversized bodies, and opaque storage errors passed. Real SQLite persisted after reopening, rolled back abandoned transactions, decoded corrupt rows as failures, and enforced the quota under concurrent creates. Workflow fakes proved no partial state before/during commit failures. |
| Notes Chromium and WebKit | Both completed list/new/detail/edit/delete flows with JavaScript disabled and blocked. A delayed Gyral preview adopted current inputs, escaped injection-shaped text, updated counts, and resumed from current values after reconnect. FormData contained only native fields; an inserted shadow control had no outer form owner. CSP, 320px layout, and 200% text scaling passed. |
| Notes process restart | Same SQLite file and cookie retained the saved record. Old CSRF proofs returned a 403 form preserving the draft with a new proof; saved content stayed unchanged. Logs omitted note content and capability/proof values. |
| SQL metadata | `python3 tools/check-notes-schema.py` passed against an empty temporary database created from migrations; offline builds used the five checked query descriptions. |
| Notes domain mutation check | 33 mutations examined: 27 caught, six unable to compile, none survived. Four properties use 128 cases and seed 731, covering title/body normalization, identifier round trips, and capability encoding/redaction. |
| Client model mutation check | All 12 generated mutations examined: nine caught, three unable to compile, none survived. The command selected only `model::tests` in a temporary copy, so it did not need ignored build artifacts. |
| `make client-docker-smoke` | Built the Linux amd64 image with an independent Rust target cache. Native create/update/delete returned 303. Replacing the container with its named volume and the same notebook cookie retained ownership and the saved note. Verified assets/CSP and uncached misses passed. UID was 10001; Node/Deno and a server bundle were absent. Both SIGTERM stops exited zero. |
| `make docker-smoke` regression | The existing SSR image still built and passed its complete container smoke after the workspace member was added. |
| Visual review | Inspected the greeting/counter and notes editor desktop/mobile Chromium screenshots; forms, preview, controls, and navigation remained readable without horizontal clipping. |

The client image ID was
`sha256:afe02a955fb1e25f2d7732f6a3ad483475e9564b77c9b66f152676af6a8c880b`.
The mutation commands were:

```sh
cargo mutants --file examples/askama-client-app/src/model.rs --package gyral-askama-client-example --test-workspace=false --output .local/client-model --timeout 60 -- model::tests
cargo mutants --package gyral-example-notes-domain --test-workspace=false --output .local/notes-mutants --timeout 60
```

Logs and browser screenshots are retained under ignored `.local` paths. Notes checks used temporary databases; `.local/notes-ci.log`, `notes-docker.log`, `notes-ssr-regression.log`, and `notes-mutants.log` record this extension. WebKit's screenshot helper injects an inline stylesheet, so screenshots were captured after the clean application CSP assertions. This is a synthetic integration example with verified SQLite persistence.
Account authentication, fetch-based API workflows, complex props, other browser
engines, client-example load budgets, and client release distributions were not
verified.
See [the design](design/askama-client.md) for its rendering and asset boundaries.

## Fresh-isolate fixture baseline

The final local release run used the 105,636-byte server bundle and returned
6,506 bytes for its last synthetic request. No other verification jobs from this
repository were running during sampling. The host was not dedicated to this
measurement; scheduling and background activity can change the results.

| Measurement | Observed time |
| --- | --- |
| Worker/platform startup | 0.945 ms |
| First render | 58.295 ms |
| Median of 50 renders | 27.986 ms |
| p95, nearest-rank sample estimate | 47.746 ms |

Render timings include isolate creation, bundle evaluation, encoding, Gyral
rendering, and output conversion. Startup file reads/digest verification and HTTP
transport are excluded. The median averages the two central samples. This is
one fixture baseline, not an application budget, concurrency test, or throughput
estimate. No pool or snapshot comparison was performed; the design and command
are documented in [assets and CSP](design/assets-and-csp.md).

## Operating and release evidence

The operating extension passed the complete local gate on October 11, 2026.
Rust 1.99.0 is both the pinned build toolchain and tested MSRV on
`x86_64-unknown-linux-gnu`. The libraries remain unpublished. Read
[the operating design](design/operations-and-release.md) for the host boundary,
fixture budgets, source distribution, and pre-1.0 compatibility policy.

| Check | Observed result |
| --- | --- |
| `make ci` | Passed strict formatting/Clippy/docs, 61 nextest tests, four doctests, all SSR/client/notes Chromium and WebKit checks, startup/schema/domain builds, four supply-chain regression tests, deny, and audit. The existing scoped paste advisory exception remains. |
| Domain mutation regression | Re-ran 89 mutations: 75 caught, fourteen unable to compile, none survived. |
| Library telemetry | Tests distinguished every safe error category, running versus stopped admission, and caller-recorded success/error/cancellation counters and histograms. Metric labels omitted private props and page IDs. |
| Readiness | A real bundle probe passed before bind. Correctly hashed bundles that threw or looped failed before HTTP startup with redacted categories. Native tests rejected missing, stale, failed, stopped-worker, and draining readiness evidence. |
| Operating smoke | Active-request SIGTERM and SIGINT both withdrew readiness, rejected a new page, completed the accepted page, and confirmed renderer thread joins. Request IDs were generated rather than echoed. Disabled OTLP produced no missing-layer warnings. JSON logs and real HTTP/protobuf OTLP payloads retained request/trace correlation while omitting private query/cookie/JavaScript values. |
| Native archive | Downloaded bytes matched the upstream V8 150.4.0 release asset's size and SHA-256. Root and Docker builds used the pinned local archive. Regression checks rejected corruption and incorrect length; policy checked the lockfile and forced local archive configuration. |
| SSR Docker | Current non-root Linux amd64 image passed SSR/assets/CSP/errors, generated IDs, readiness/metrics, Node absence, and SIGTERM with explicit worker joins. |
| Askama Docker regression | Client image retained native CRUD, notebook ownership, and SQLite data across replacement; assets/CSP, UID 10001, absent Node/Deno/V8, and SIGTERM passed. |
| Local release assembly | Produced source and reference-container binary archives, lockfiles, a descriptor, native pin, and checksums. The SSR CycloneDX inventory contained 286 components; the complete Gyral vendor inventory contained nine inputs. Both passed the official CycloneDX 1.5 JSON schema. |
| Extracted release artifacts | Checksums and inventory edges/native digest passed. Source policy passed after extraction, with tracker records, databases, caches, and generated assets excluded. The extracted binary served SSR and readiness and stopped on SIGTERM. Available license texts, including the target-conditional TLS data agreement, were included. |

The current SSR image ID is
`sha256:884945af95149a61605344e813b1d2546a3329e3ff823d7bb5a38686ff07a849`.
The client regression image ID is
`sha256:11d25bbde0385d26a0e11b59dd98aede252918b9868e546c46c42a3f2fa72165`.
Local reports use `.local/operating-*`; assembled artifacts are under
`.local/release/0.1.0`. No artifacts were published, and no hosted workflow ran.

### Sustained fresh-isolate fixture

The final `make load` run rebuilt the release binary from the current source,
warmed for ten seconds, then ran four closed-loop clients for 60.100 seconds.
Other repository builds and browser checks had finished before sampling.
The host was not dedicated; other applications and scheduling can affect results.
Every sampled response contained its own synthetic name and returned HTTP 200.

| Measurement | Observed result |
| --- | --- |
| Successful sampled requests | 2,930 |
| Throughput | 48.752 responses/second |
| p50 / p95 / p99, nearest rank | 79.313 / 97.765 / 132.051 ms |
| Maximum sampled process RSS | 40.660 MiB |
| RSS growth, final versus first window maximum | 1.309 MiB |
| Maximum sampled queue occupancy | Four of sixteen positions |
| Overload burst | A separate 64-request burst shed work with 503 and then recovered to 200. |

All documented fixture budgets passed. These are single-host, one-minute
synthetic observations, not application SLOs, an open-loop capacity limit, a
memory cap, or a long-duration leak proof. No pooling or snapshot optimization
was introduced.

## Limits of this evidence

Dependency checks passed with the single documented `paste` maintenance
[exception](security.md). Duplicate transitive versions produced policy warnings. The operating
extension verifies the supported native archive, local SBOMs, and one sustained
fixture profile. Other targets, application load budgets, and broader Gyral
features remain unverified.

The tested [resource policy](design/runtime-limits.md) uses development defaults:
a two-second render deadline, 64 MiB managed V8 heap budget, and two-second
shutdown deadline. It interrupts JavaScript but cannot safely destroy a native
thread stalled in engine initialization or cleanup. Shutdown expiry bounds the
caller's wait without certifying that native cleanup has finished. The managed
heap budget excludes temporary termination headroom, external storage, and
native allocations. Fatal engine errors and process OOM remain outside recovery.
Only trusted app bundles are supported.

The example's `/health` check establishes HTTP reachability, not renderer
readiness; `/readyz` supplies recent real-bundle evidence and lifecycle checks.
Fresh isolates remain the default. The one-minute fixture throughput above is
not an application capacity claim, and no pooling comparison was performed. Browser evidence covers the synthetic feature fixture; it
does not establish compatibility with complete applications, server `formAction`,
split chunks, dynamic styles, streams, or future framework releases. Startup
hashes do not authenticate a build author. Asset retention during rolling
releases has not been verified.

The evidence in this section predates the public repository. GitHub Actions has
since passed on a hosted runner, as recorded in the dependency refresh above. No
package publication, image push, or deployment has been made. Beads
tracks the resource, compatibility, and operating evidence under
`gyral-rs-xun`, optional snapshot/pool evaluation under `gyral-rs-xun.4`, and
advisory removal under `gyral-rs-83t`.

## Published Gyral and runtime evaluation

The October 11, 2026 update pins npm `@gyral/core` and `@gyral/ssr` to `0.3.1`,
removes the nine local prerelease archives and file overrides, and regenerates
the frozen lockfile with registry SHA-512 integrity. Both build manifests now
require that exact release. Earlier sections preserve prerelease evidence;
their bundle sizes, image IDs, and timings are historical.

The full local `make ci` gate passed with 64 Rust nextest tests, four doctests,
the three new snapshot isolation/resource/lifecycle checks, operating and OTLP
probes, all Chromium/WebKit SSR/client/notes suites, architecture/SQL policy,
and dependency checks. The release-only measurement test is intentionally ignored
by normal CI and executed separately by `make evaluate-runtime`. The final
strict Clippy/format/policy check also covers that benchmark's revised client
count. Domain mutation testing again found 89 mutants: 75 caught, 14 unviable,
and zero survivors.

The completed runtime comparison uses the published fixture in eighteen isolated
release test processes and records 6,161 successful concurrent-window renders.
For one worker and one active client, median run-level throughput increased from
23.72 to 47.49 renders/second; p95 decreased from 60.58 to 35.06 ms. Median sampled
peak process RSS was 38.21 versus 39.76 MiB, including preparation. Three runs per
candidate and worker count, phase profiling, startup costs, host load, the earlier
failed queued attempt, and the adoption decision are recorded in
[runtime evaluation](design/runtime-evaluation.md). These are shared-host fixture
observations, not application SLOs or a production snapshot API. Fresh isolates
and per-request bundle evaluation remain the public default.

Both npm-based container images passed their smoke checks. Local release
assembly and extracted-artifact checks passed with 287 SSR inventory components
and a connected three-package npm production inventory. Binary archives include
available npm license texts; source archives exclude vendor directories. The
updated release descriptor under `.local/release/0.1.0/release.json` records the
current compiler-input fingerprint and tested SSR image rather than relying on
the historical IDs above. Both updated inventories passed the official
CycloneDX 1.5 JSON schema. Complete-application snapshot artifacts and lifecycle
validation are tracked in `gyral-rs-xun.5`. No hosted CI run or external
publication is claimed.
