# Local development

Run commands from the repository root. Rust is pinned to 1.99.1, edition 2024;
Node 24.22.0 and pnpm 12.14.0 build the example bundles. Python 3.11+ runs policy
checks. Install cargo-nextest 0.9.148, cargo-deny 0.20.2, cargo-audit 0.22.2,
cargo-cyclonedx 0.5.9, and SQLx CLI 0.9.0 for
the full gate. The CLI and query macros use the same SQLx 0.9.0 release.

```sh
rustup show
corepack enable
corepack prepare pnpm@12.14.0 --activate
cargo install cargo-nextest --version 0.9.148 --locked
cargo install cargo-deny --version 0.20.2 --locked
cargo install sqlx-cli --version 0.9.0 --locked --no-default-features --features sqlite
cargo install cargo-audit --version 0.22.2 --locked
cargo install cargo-cyclonedx --version 0.5.9 --locked
pnpm install --frozen-lockfile
make prepare-native
pnpm exec playwright install chromium webkit
```

Do not replace another project's toolchain or databases. Beads state is local;
fresh checkouts initialize their own tracker using the command in [AGENTS.md](../AGENTS.md).
`bd init` may create a local metadata commit. Do not interpret it as permission
to commit implementation files or configure a remote.

## Dependency updates

Lockfiles make native builds and generated bundles repeatable. Example and tool
manifests also use exact versions. Published libraries use caret requirements
whose lower bound is the verified version, plus an exact `deno_core` pin, so that
consumers can resolve them alongside other crates; the
[release runbook](releasing.md) records that decision. None of these update
themselves when new versions appear. Review current registry releases, update
the manifests, refresh both lockfiles, then run `make ci`, `make client-docker-smoke`,
`make release-evidence`, and `make load` before adopting the new set.

Dependabot checks Cargo and npm weekly and CI actions monthly, and proposes a version only once
it is two weeks old (its `cooldown`); pnpm's `minimumReleaseAge` refuses younger npm versions.
Cargo has no age gate, so check crates.io publish dates before a lockfile refresh
([security](security.md)). The same two weeks apply to the Rust, Node, and pnpm
toolchains, CI actions, and the Dockerfiles' base images. Date a toolchain by its
release on static.rust-lang.org or in nodejs.org's `index.json`, not by the commit
date in its version string. Official image tags are rebuilt in place, so a tag's
current digest can be younger than its version; pin the digest of a push that is two
weeks old, which the history of
[docker-library/repo-info](https://github.com/docker-library/repo-info) records for
each tag, or a dated Debian tag. Related packages
share a proposal: SHA-2 with HMAC, the Serde family, the embedded runtime, and
the Gyral packages. CI actions also share a proposal. SHA-2 and HMAC must use
compatible digest traits; an isolated major upgrade can fail even when the
combined upgrade works. HMAC 0.13 constructs keys through `KeyInit`, as shown
in its [official examples](https://docs.rs/hmac/0.13.0/hmac/).
Review `deno_core`, its V8 dependency, and the verified
native archive together. An older transitive version may be required by an
upstream package; use `cargo tree --invert <package>` to find its owner rather
than forcing an incompatible override.

pnpm 12 reads build permissions from [pnpm-workspace.yaml](../pnpm-workspace.yaml).
Only esbuild may run an installation script. Its `minimumReleaseAge` holds every
version for two weeks; the two exact Gyral 0.3.7 versions are exempt because
their owner-approved npm release was already verified. Future Gyral versions
still pass through that gate. Docker builds, source distributions, and the
release fingerprint include this configuration.

## Commands

| Command | What it checks or runs |
| --- | --- |
| `make prepare-native` | Download and verify the pinned Linux x86_64 V8 archive before raw Cargo builds. |
| `make operating` | Isolated startup readiness, metrics/IDs, JSON/OTLP redaction, W3C parent correlation, and active-request SIGTERM/SIGINT shutdown. |
| `make load` | Release fixture: ten-second warmup, sixty-second four-client profile, RSS/latency/queue budgets, burst shedding, and recovery. |
| `make release-evidence` | Build/check the reference container and prepare local source/binary archives, SBOMs, lockfiles, descriptor, and checksums. |
| `make policy` | Pinned skill hashes, registry lockfile integrity, local doc links, crate boundaries, and publishing metadata. |
| `make package` | `cargo publish --dry-run` for the four crates in dependency order, package content and size bounds, and an offline docs.rs build without the native archive. |
| `make check` | Policy, Rust formatting, and strict Clippy across every target. |
| `make test` | Build bundles, run nextest, and compile Rust documentation tests. |
| `make build` | Build both examples' assets and Rust executables. |
| `make demo` | Start the Axum example on 127.0.0.1:3037. |
| `make browser` | Start an isolated example and verify SSR and hydration in Chromium and WebKit. |
| `make ci` | Frozen install, check, tests, Rust docs, both builds, artifact startup checks, SSR/client/notes browser suites, SQL metadata verification, operating/supply-chain regression checks, and deny/audit dependency policy. |
| `make docker` | Build the non-root example image; no push or deployment. |
| `make docker-smoke` | Build and check container SSR, assets, non-root execution, Node absence, and graceful stop. |
| `make measure` | Release-mode fixture baseline: 50 sequential renders with fresh isolates; no pooling or load claim. |
| `make evaluate-runtime` | Verify the test-only snapshot candidate, then compare fresh/snapshot startup and bounded worker counts in isolated release test processes. |
| `make client-build` | Build browser-only Gyral assets and the Askama executable, without building V8. |
| `make client-demo` | Start Askama pages and browser-only components on 127.0.0.1:3038. |
| `make client-browser` | Verify client artifacts, startup rejection, dependency isolation, first rendering/fallbacks, and notes with and without JavaScript. |
| `make client-docker` | Build the separate non-root Askama client example image. |
| `make client-docker-smoke` | Check pages, CRUD, persisted ownership across container replacement, assets, runtime contents, and graceful SIGTERM. |

The supported embedded build is Linux x86_64 GNU with Rust 1.99.1. Run
`make prepare-native` before a raw Cargo command that builds V8. Root Make targets
prepare automatically. The reviewed size/SHA-256 pin lives in `tools/v8-archive.json`;
Cargo consumes the verified local archive through root configuration. A corrupt
cache fails verification. Other host projects must adopt the same preparation and
pin because Cargo does not inherit a dependency repository's configuration.
Read [operations and release](design/operations-and-release.md) for target/MSRV,
semver, artifact inventory, and upstream integrity limits.

## Recovery

If the real Gyral integration test cannot find `dist/manifest.json` or `dist/server.js`, run
`pnpm build:example`. A JavaScript error is a typed render failure, not a reason
to install Node in the server image. Check the [bundle contract](design/bundle-contract.md)
and run the native tests before changing the runtime's capabilities.

The example honors `GYRAL_EXAMPLE_BIND` and `GYRAL_EXAMPLE_DIST` at startup.
It verifies all artifacts before binding HTTP. On a digest mismatch or missing
file, rebuild the complete pair with `pnpm build:example`; do not hand-edit hashes
to make a mixed build pass. The hashed browser file is the only public asset route.
`/health` and `/livez` report HTTP reachability. `/readyz` uses recent bundle
probe evidence, worker state, and host admission; `/metrics` exposes bounded
Prometheus metrics. Ctrl+C and SIGTERM withdraw readiness, drain accepted pages,
and join the worker. Logs are JSON and omit request props and thrown text.
Set `GYRAL_OTLP_ENDPOINT` to a complete HTTP/protobuf `/v1/traces` endpoint for
optional trace export. Allow fifteen seconds for container termination.

The [Askama client example](design/askama-client.md) uses `GYRAL_CLIENT_BIND`
and `GYRAL_CLIENT_DIST`, with port 3038 by default. Run `pnpm build:client-example`
to repair a missing or mixed client artifact set. It requires no server bundle
or V8 archive; its two exact asset URLs serve JavaScript and external CSS.
Run `make client-build` for this example alone. `make build`, `make test`, and
`make ci` cover both examples and therefore also build the embedded renderer.

## Notes data and SQL metadata

Open <http://127.0.0.1:3038/notes> after `make client-demo`. The notes database
defaults to `.local/notes.sqlite`, resolved from the project directory.
`GYRAL_NOTES_DB` accepts another file path. Startup creates its parent, applies
migrations, and fails before HTTP binding if the database cannot open or migrate.
The cookie identifies this browser's notebook for 30 days. Keep it to return to
the same records. Restarting preserves notes but expires already-open forms;
submitting one returns the preserved draft and a fresh CSRF proof.

Use synthetic content. To reset the local default, stop the server and remove
`.local/notes.sqlite` plus its `-wal` and `-shm` sidecars. Do not change another
application's database. For a simple local backup, stop the server before copying
the database; copying a live file alone can miss WAL changes. Browser and native
tests use isolated temporary databases, not the developer's notebook.

Offline builds read checked `.sqlx` descriptions, which contain schema metadata,
not stored notes. After changing SQL or migrations, create an empty temporary
SQLite database from the migrations, set `DATABASE_URL` to it and
`SQLX_OFFLINE=false`, and run `cargo sqlx prepare --workspace -- --all-targets`.
Never prepare against application data. Verification automates the empty-schema
setup:

```sh
python3 tools/check-notes-schema.py
```

For a retained local Docker notebook:

```sh
make client-docker
docker volume create gyral-notes-data
docker run --rm --publish 127.0.0.1:3038:3038 --mount type=volume,source=gyral-notes-data,target=/app/data gyral-rs:askama-client
```

Stop the dev server first if it holds port 3038. The image runs as UID 10001 and
stores SQLite in `/app/data/notes.sqlite`; the named volume survives container
replacement. Keep both the volume and notebook cookie. The container smoke uses
its own volume, verifies create/reopen/update/delete, then removes it.

## Mutation checks

For the Rust skill's domain mutation check, install cargo-mutants 27.1.0 and run:

```sh
cargo mutants --file 'crates/domain/src/*.rs' --package gyral-renderer-domain --test-workspace=false --output .local --timeout 60
cargo mutants --package gyral-example-notes-domain --test-workspace=false --output .local/notes-mutants --timeout 60
```

The tool uses a temporary copy; `.local` keeps reports out of Git. A surviving
mutation needs a test or a documented reason it cannot change behavior.
