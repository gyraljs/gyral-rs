# Working in gyral-rs

This repository embeds Gyral server rendering in Rust through `deno_core`. Read the
contract and verification evidence before extending the packages.

## Repository map

- [README](README.md): package purpose and first run.
- [Contributing](CONTRIBUTING.md): public reports, local tracking, checks, and DCO sign-off.
- [Security policy](SECURITY.md) and [conduct](CODE_OF_CONDUCT.md): private reporting and community expectations.
- [Architecture](ARCHITECTURE.md): dependency direction and request lifecycle.
- [Design decisions](docs/design/index.md): the bundle contract and accepted defaults.
- [Runtime limits](docs/design/runtime-limits.md): deadlines, heap budgeting, cancellation, and shutdown.
- [Runtime evaluation](docs/design/runtime-evaluation.md): test-only snapshot and worker comparisons, evidence, and adoption limits.
- [Assets and CSP](docs/design/assets-and-csp.md): verified builds, caching, policy, and compatibility scope.
- [Operations and release](docs/design/operations-and-release.md): readiness, telemetry, signal drain, load budgets, native inputs, and distribution.
- [Askama client example](docs/design/askama-client.md): Rust templates with browser-only components and no embedded renderer.
- [Development](docs/development.md): tools, commands, testing, and recovery.
- [Engineering](docs/engineering.md): skill provenance and functional Rust interpretation.
- [Writing](docs/writing.md): documentation audience and revision conventions.
- [Security](docs/security.md): trusted code, untrusted data, and current limits.
- [Quality](docs/quality.md) and [verification](docs/verification.md): checks and their evidence.
- [Harness reference](docs/references/openai-harness-engineer.md): the owner-supplied reference.

## Track every task in Beads

Run `bd prime` at session start and after compaction. Use `bd ready`, inspect the
relevant issue, and claim it before implementation. Record discovered work in
Beads and close completed issues with verification evidence. Beads owns task
status and dependencies; documents own requirements and design. Do not add a
Markdown backlog, execution checklist, or another task tracker.

Initialize a fresh local tracker with `bd init --prefix gyral-rs --skip-hooks
--skip-agents --non-interactive` when needed. Do not overwrite this guide or the
pinned skills. Local databases, account records, and issue exports stay out of Git.
Hooks are not installed; run `bd prime` explicitly. Beads grants no Git authority.
Follow the current session's authorization for commits, remotes, pushes, and releases.

## Apply the requested skills

Read [Rust FP](.agents/skills/rust-fp/SKILL.md) for Rust changes,
[Sense of Style](.agents/skills/sense-of-style/SKILL.md) for all project writing,
and [Beads](.agents/skills/beads/SKILL.md) for tracking. Their source commits and
file hashes are pinned in [skills.lock.json](skills.lock.json). Update them
explicitly; do not edit copied instructions in place. Never copy a copyrighted
book or private reference into this repository.

## Preserve the boundaries

The domain crate is pure and cannot depend on V8, Tokio, Axum, I/O, or a clock.
The renderer owns the embedded runtime and worker lifecycle. The Axum adapter
maps rendering results to HTTP; applications own routing, access checks, data
loading, response policy, and mutations. JavaScript receives only approved DTOs.
Accept trusted app bundles; do not claim this is an untrusted-code sandbox.

## Verify and report accurately

Use `make check`, `make test`, and the relevant browser/build checks. Use `make ci`
for the complete local gate. Follow the Rust skill's final self-review. Update
docs when behavior or architecture changes. Distinguish owner requirements,
design defaults, tested behavior, and remaining work. Never claim unrun checks.
Exclude credentials and datasets, and never add co-author trailers.
