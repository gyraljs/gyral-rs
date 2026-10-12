# Contributing to gyral-rs

Contributions should help Rust applications render Gyral components safely or
make the browser-only example easier to understand. The workspace is pre-1.0;
discuss substantial API, bundle-contract, runtime, or deployment changes in a
[GitHub issue](https://github.com/gyraljs/gyral-rs/issues) before implementing them.

You can report a bug or propose a change without installing Beads. Maintainers
use local Beads records for implementation status and dependencies; GitHub
issues provide public reports and design discussion.

## Set up a checkout

The tested embedded target is Linux x86_64 GNU. Install Rust 1.99.0, Node
24.21.0, pnpm 12.11.2, Python 3.11 or later, and Make. The
[development guide](docs/development.md) lists exact versions and installation
commands for nextest, cargo-deny, cargo-audit, cargo-cyclonedx, SQLx CLI, and
browser dependencies required by the complete gate.

Fork the repository on GitHub, clone your fork, and create a focused branch.
Run these commands at its root:

```sh
corepack enable
corepack prepare pnpm@12.11.2 --activate
pnpm install --frozen-lockfile
pnpm exec playwright install --with-deps chromium webkit
make prepare-native
```

The first embedded build downloads and verifies the pinned V8 archive. For the
Askama example alone, use `make client-build`; that dependency graph excludes V8.
Keep development databases and build output in their ignored local directories.

## Read the boundaries before changing code

[AGENTS.md](AGENTS.md) maps the repository instructions. Read
[architecture](ARCHITECTURE.md) and the relevant document in
[design](docs/design/index.md) before changing an established contract.

The domain crate remains a pure leaf. The renderer owns V8, its execution thread,
watchdog, cancellation, and shutdown. The Axum adapter maps rendering results;
host applications own routing, authorization, data loading, and mutations.
Reviewed bundles are supported; arbitrary third-party JavaScript is outside the
contract. The Askama example preserves native form controls and submission.

Apply the pinned skills for Rust, writing, and tracking described in
[engineering](docs/engineering.md). Do not edit the copied skill instructions
in place. Propose deliberate source and hash updates when a skill needs changing.

For local implementation work, initialize your own tracker if this checkout
has none, then claim the relevant task before changing code. Install `bd` using
the [Beads installation guide](https://github.com/gastownhall/beads) if needed:

```sh
bd init --prefix gyral-rs --skip-hooks --skip-agents --non-interactive
bd prime
bd ready
```

Create a descriptive task if the work is new. Keep task status in Beads and
designs in documents. Close completed tasks with evidence. Never commit your
local database, task exports, account records, credentials, or notebook data.
Read [AGENTS.md](AGENTS.md) for the complete tracking policy.

## Verify the change

Choose checks that exercise the changed behavior:

| Change | Relevant checks |
| --- | --- |
| Documentation | `make policy`, local links, command and claim consistency, staged whitespace. |
| Rust library or example behavior | `make check`, `make test`, then `make ci` before submitting. |
| SSR bundles, assets, or hydration | `make browser` and artifact checks in `make ci`. |
| Askama templates, components, or notes | `make client-browser`, including the JavaScript-disabled form flow. |
| SQL or migrations | Prepare against an empty temporary database and run `python3 tools/check-notes-schema.py`. |
| Runtime, native inputs, or containers | Relevant Docker smoke checks, operating checks, and release evidence. |

Add a regression test for a behavioral bug and meaningful tests for new behavior.
Pure domain tests use property-based checks; application workflows use port
fakes; persistence and lifecycle boundaries use real integration checks. Do not
add tests that merely repeat an implementation or assert incidental formatting.

Dependency upgrades preserve lockfile integrity. Published libraries use caret
requirements, except the exact `deno_core` pin; examples and tools keep exact
pins. The [release runbook](docs/releasing.md) explains why. A native upgrade
also needs a reviewed V8 archive size/digest and compatible target evidence.
Changing the Gyral version requires matching server/browser artifacts and a
compatibility fixture. Update documentation when behavior or architecture changes.

## Submit a pull request

Explain the concrete problem and resulting behavior. Include the commands you
ran and their results, relevant compatibility changes, and remaining limitations.
Use the [pull request template](.github/pull_request_template.md); do not claim
checks you did not run. Keep each PR focused so its behavior can be reviewed.

Do not include personal datasets, addresses, secret material, local build
artifacts, or Beads records. Review `git diff --cached` before committing. Never
add co-author trailers. All proposed code changes must be compatible with the
project's [MIT License](LICENSE) and preserve third-party attribution.

### Developer Certificate of Origin

Sign off contribution commits under the
[Developer Certificate of Origin](https://developercertificate.org/), following
the Gyral organization's contribution convention. Sign-off certifies that you
have the right to submit the contribution under its license.

```sh
git commit -s -m "Describe the changed behavior"
```

Use your contributor identity. Add a missing sign-off to your own unpublished
commit with `git commit --amend -s --no-edit`; do not rewrite shared history to
repair it. A DCO sign-off is separate from cryptographic commit signing.

## Conduct and security

Participation follows the [Code of Conduct](CODE_OF_CONDUCT.md). Report security
problems privately using [SECURITY.md](SECURITY.md), rather than posting an
exploit, credentials, or private data in a public issue.
