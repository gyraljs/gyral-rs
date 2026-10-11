# Engineering conventions

The owner requested functional Rust, reader-centered writing, Beads, and the
[OpenAI harness reference](references/openai-harness-engineer.md). The scaffold
implements these through a short [repository map](../AGENTS.md), linked designs,
typed package boundaries, executable examples, and mechanical quality checks.
The article's Markdown execution plans are replaced by Beads, as requested.
No scheduled agents, remote publishing, or automatic merge behavior is configured.

## Pinned skills

| Skill | Source copy | Applied to |
| --- | --- | --- |
| Rust FP | [.agents/skills/rust-fp](../.agents/skills/rust-fp/SKILL.md) | Rust models, error tracks, dependency injection, infrastructure, and tests. |
| Sense of Style | [.agents/skills/sense-of-style](../.agents/skills/sense-of-style/SKILL.md) | README, architecture, runbooks, API docs, and other project writing. |
| Beads | [.agents/skills/beads](../.agents/skills/beads/SKILL.md) | All task tracking and session recovery. |

[skills.lock.json](../skills.lock.json) records source repositories, commits,
dirty-source flags, and hashes for copied files. `make policy` rejects modified
copies. Update sources and the lock deliberately; do not patch a copied skill.
The writing skill's copyrighted/private source material was not copied.

## Functional Rust in a library workspace

The pure domain has its own crate. The two application-facing libraries
re-export its types rather than accepting raw page strings or unvalidated request
objects. The runtime implements a `RenderService` capability; the HTTP adapter
and its tests can use a hand-written implementation without starting V8.

Library APIs return named errors. Infrastructure diagnostics stay in typed
renderer errors and are omitted from default HTTP responses. Production code
forbids unsafe code, unwraps, expects, panics, unchecked indexing, and unbounded
channels through workspace lints and Clippy configuration. Test fixtures may use
documented `expect` calls. The release profile preserves unwind and integer
overflow checks.

The example illustrates integration and host-owned operations. Read
[operations and release](design/operations-and-release.md) for the complete Rust
production-requirement review, readiness, redacted logs/traces, metrics, signal
shutdown, supported build policy, and sustained fixture budgets. This evidence
does not certify an authenticated application or its deployment workload.

The skill's library checklist is applied to unpublished packages: public enums
remain exhaustive across this workspace, and `RenderService` stays open for
application fakes and alternative implementations. A published semver baseline does not exist yet. The initial
pre-1.0 compatibility policy requires API comparisons once a baseline exists. The validated request is also the explicit JSON wire contract;
Serde deserialization routes through its constructor. There are no payment or
WASM workflows in this scaffold. The notes example demonstrates database
workflows with pure domain types, injected transaction ports, hand-written
failure fakes, and real SQLite rollback/persistence/access checks. SQLx macros
check queries against an empty migrated schema; `.sqlx` metadata supports offline
builds and is verified in CI. The fixed-sample measurement
CLI reports release-mode fixture timings; it does not operate application data.

The example's configuration is parsed at startup into a socket address and an
asset path. Missing settings use documented development defaults; a malformed
bind address fails startup. HTTP traces record route patterns rather than
query strings. JavaScript errors remain inspectable as typed values, but default
logs omit their potentially private text. The SSR host now supplies telemetry, readiness, and bounded signal drain.
Application authentication, workload budgets, and process containment remain
host responsibilities.

Resource policy remains pure: `RenderLimits` validates durations and byte budgets
without reading a clock. The runtime uses a monotonic clock in its infrastructure
shell. A small leaf controller protects admission and the active V8 handle across
the worker and watchdog; it never holds a lock across await or a V8 call.
Request outcomes use explicit terminal states and preserve the first interruption.

The Rust skill's virtual-time test rule has one infrastructure interpretation:
Tokio paused time cannot drive a watchdog on an OS thread while synchronous V8
blocks its own executor. Native interruption tests therefore use real deadlines,
condition-variable handshakes, and nextest process timeouts. They do not use
arbitrary sleeps. Pure policy tests remain deterministic and use a fixed seed.
The simulated upstream panic exists only in the unit-test build and proves
isolate disposal and subsequent worker recovery. Process aborts are not caught.

Build metadata follows the same parse-once boundary: bounded JSON enters private
validated fields, and digests use a fixed-size type with canonical text encoding.
Base64 is a pure encoding dependency in the domain. SHA-256 verification stays
in the renderer shell; file reads and HTTP policy remain host concerns.

The compatibility fixture uses Gyral's own view layer and command/store APIs,
as requested. It stays in JavaScript modules with the existing esbuild pipeline;
the pure-Lit skill does not apply to another view framework. Modern CSS guidance
is applied to layered, theme-aware styles, semantic controls, and native layout.
Browser checks verify both themes, narrow layout, real CSP blocking, preserved
form input, and declared-private-field redaction. Typed Rust startup failures
and HTTP contracts are checked independently of browser interaction.
