# rust-fp-skill — Functional, Railway-Oriented Rust

[![Claude Code Skill](https://img.shields.io/badge/Claude_Code-Skill-d97757?logo=anthropic&logoColor=white)](https://code.claude.com/docs/en/skills)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B_edition_2024-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Paradigm](https://img.shields.io/badge/paradigm-functional-8A2BE2)](https://fsharpforfunandprofit.com/series/thinking-functionally/)
[![ROP](https://img.shields.io/badge/errors-railway--oriented-orange)](https://fsharpforfunandprofit.com/rop/)
[![Types](https://img.shields.io/badge/illegal_states-unrepresentable-success)](https://fsharpforfunandprofit.com/series/designing-with-types/)
[![Testing](https://img.shields.io/badge/testing-property--based-blueviolet)](https://fsharpforfunandprofit.com/series/property-based-testing/)
[![unwrap](https://img.shields.io/badge/unwrap-banned-red)](#hard-rules)
[![panic](https://img.shields.io/badge/panic!-banned-red)](#hard-rules)
[![unsafe](https://img.shields.io/badge/unsafe-forbidden-red)](#hard-rules)
[![References](https://img.shields.io/badge/references-13_files-informational)](#whats-inside)
[![License](https://img.shields.io/badge/license-CC_BY_4.0-lightgrey)](LICENSE)

A [Claude Code skill](https://code.claude.com/docs/en/skills) that makes an AI coding agent build **every** Rust program — services, CLIs, libraries, workspaces — as a pure functional core wrapped in a thin effectful shell, with every failure travelling on a typed error track. Railway-oriented error handling, newtype-and-enum domain modeling, parse-don't-validate boundaries, trait-based dependency injection, property-based testing, and a production checklist treated as the definition of done.

> **Proof repo:** [rust-fp-skill-examples](https://github.com/mikezupper/rust-fp-skill-examples) — a full commerce API built strictly with this skill. See [below](#proof-repo).

The design philosophy is Scott Wlaschin's ([F# for Fun and Profit](https://fsharpforfunandprofit.com)) and Alexis King's *Parse, Don't Validate*. Rust's `Result`, enums, ownership, trait system, and — crucially — **cargo itself** are its native realization.

---

## Table of contents

- [Motivation](#motivation)
- [The architecture is enforced by the build](#the-architecture-is-enforced-by-the-build)
- [Principles](#principles)
- [Hard rules](#hard-rules)
- [Concept → Rust realization](#concept--rust-realization)
- [What doesn't map](#what-doesnt-map)
- [What's inside](#whats-inside)
- [Proof repo](#proof-repo)
- [Installation](#installation)
- [How the skill works](#how-the-skill-works)
- [How to best leverage it](#how-to-best-leverage-it)
- [Version policy](#version-policy)
- [Sources & credits](#sources--credits)

---

## Motivation

AI agents write plausible Rust by default. It compiles, it passes the demo, and it is quietly wrong in exactly the same nine places every time:

| Default the agent reaches for | Where correctness leaks out |
|---|---|
| `unwrap()` / `expect()` in the happy path | A partial function; the error track was never designed |
| `-> Result<T, Box<dyn Error>>` | Callers cannot match on failure; the error is prose in a trench coat |
| `anyhow` everywhere, including libraries | Erases the error taxonomy at exactly the layer that needed one |
| `Result<T, String>` | Errors are data. Strings are not data |
| `struct Order { email: String, sku: String }` | Every field is `String`; nothing is validated; argument order is a coin flip |
| `price: f64` | Money is not a float. It never was |
| `Utc::now()` inside a workflow | Untestable, unreproducible, unmockable — a hidden effect in pure code |
| Validation scattered through the call graph | The same check runs five times and is still missed once |
| `Arc<Mutex<AppState>>` threaded through everything | Shared mutable state, re-invented, with worse ergonomics |

The functional-programming tradition solved every one of these, and named the solutions:

- **[Railway Oriented Programming](https://fsharpforfunandprofit.com/rop/)** — errors as values on a second track, composed rather than caught.
- **[Designing with Types](https://fsharpforfunandprofit.com/series/designing-with-types/)** — make illegal states unrepresentable, so the compiler is the test suite.
- **[Parse, don't validate](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/)** — decode untrusted input exactly once, at the boundary, into a type that carries the proof.
- **[A Recipe for a Functional App](https://fsharpforfunandprofit.com/series/a-recipe-for-a-functional-app/)** — functional core, imperative shell; effects at the edges only.
- **[Property-Based Testing](https://fsharpforfunandprofit.com/series/property-based-testing/)** — test the invariants that must hold for *all* inputs, not the three you thought of.

Rust has unusually good native support for all five. `Result<T, E>` plus `?` *is* the two-track railway with syntax. Enums plus exhaustive `match` *are* algebraic data types with compiler-checked totality. Newtypes with private fields and `TryFrom` *are* parse-don't-validate, and they compile to nothing. Ownership makes the aliasing bugs that immutability was invented to prevent structurally impossible.

None of which an agent will apply unless you make it. This skill makes it — hard rules with mechanical enforcers, decision tables, anti-pattern lists, version-verified API examples, per-area checklists, and a mandatory self-review pass.

## The architecture is enforced by the build

This is the headline claim, and it is the reason a Rust FP skill is different in kind from one written for any other language.

**In most languages, architecture is a convention you can lint for.** A layering rule is an ESLint plugin, an ArchUnit test, a grep in CI, a code-review norm. It is advisory: the import still resolves, the program still runs, and the rule holds only as long as everyone keeps caring.

**In Rust, the architecture is a property of the dependency graph, and cargo resolves it.**

| Mechanism | What it makes impossible |
|---|---|
| `crates/domain/Cargo.toml` simply does not list `tokio`, `sqlx`, `axum`, or `reqwest` | Calling the database from the domain is not a lint warning — the symbol does not exist. `use sqlx::…` in `domain` is a **resolution error**. The onion is a compile error to violate |
| `clippy.toml` `disallowed-methods` bans `chrono::Utc::now`, `uuid::Uuid::new_v4`, `rand::random` **by full path** | Hidden effects in pure code. Time, randomness, and IDs must be injected through a port, because the direct call does not build |
| `[workspace.lints.clippy]` denies `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `wildcard_enum_match_arm` | Partial functions and catch-all match arms. Adding an enum variant **breaks the build** instead of silently falling into `_ => {}` |
| `#[serde(try_from = "RawOrder")]` instead of `#[derive(Deserialize)]` on domain types | An unvalidated domain value is *structurally unconstructible* from JSON. Serde has no path to the fields; the only door is your `TryFrom`, and it returns `Result` |
| `#![forbid(unsafe_code)]` in every crate | Not `deny` — `forbid`. A downstream `#[allow]` cannot reopen it |
| `cargo-mutants` in CI | Error tracks that are declared but never exercised. It mutates the red track and fails if no test notices |

The result is that the properties this skill cares about survive contact with future contributors — including future agents — who have never read the skill. They do not need to. The build says no.

Most languages can only lint or grep for these. Rust can refuse to compile.

## Principles

1. **Railway-oriented programming.** Every operation that can fail returns `Result<T, E>` with a named, structured `E`. Errors are values on a typed track, never panics. `?` is the switch that shunts to the error track. Compose the happy path; handle failures where you have context to act.
2. **Make illegal states unrepresentable.** Newtypes for every domain primitive, enums for every "or", typestates for every lifecycle. If it compiles, it should be valid. `Option<T>` for absence — never a sentinel, never a nullable field pair.
3. **Parse, don't validate.** Untrusted data is decoded exactly once at each boundary (HTTP, DB, env, CLI, queue, file) into rich domain types via `TryFrom`. The core only ever sees types that are already correct. A domain type constructible from raw input without validation is a bug.
4. **Functional core, imperative shell.** The domain is pure functions over immutable data, in its own crate with no async runtime, no database, no web framework. Cargo enforces the dependency direction — it is not a convention, it is a compile error.
5. **Totality.** Every function handles every input in its type. No partial functions, no `unwrap` in domain code, no wildcard match arms over domain enums, no silent defaults.

## Hard rules

Non-negotiables. Every one has a mechanical enforcer in `references/scaffold.md` (`[workspace.lints]`, `clippy.toml`) and a grep in `references/code-review.md`.

| Banned | Instead |
|---|---|
| `unwrap`, `expect`, `panic!`, `todo!`, `unreachable!`, `xs[i]` in `domain`/`app` | A typed error, `.get()`, or a documented `expect("invariant: …")` at startup wiring only |
| `-> Result<T, Box<dyn Error>>` | A named `thiserror` enum, one per layer — not one god-enum |
| `-> Result<T, String>` | Errors are structured data, not prose |
| `anyhow` / `eyre` in a library or domain crate | `anyhow::Result` in `main.rs` and build scripts, nowhere else |
| Naked `String` / `i64` / `u32` across domain boundaries | Newtypes with private fields and fallible constructors (`nutype`, or `TryFrom`) |
| `price: f64` | Integer minor units in a `Cents`-style newtype, or `rust_decimal::Decimal` |
| `#[derive(Deserialize)]` on a domain type | `#[serde(try_from = "RawX")]` + a `Raw*` DTO |
| `as`-casting or transmuting external data into domain types | `TryFrom` at the boundary; conversion is fallible and says so |
| `is_paid: bool, is_shipped: bool` | `enum OrderState { Pending, Paid { … }, Shipped { … } }` |
| `card: Option<String>, paypal: Option<String>` | `enum PaymentMethod { Card(…), PayPal(…) }` |
| `_ => {}` over a domain enum | Exhaustive arms — a new variant must break the build |
| `Utc::now()`, `SystemTime::now()`, `Uuid::new_v4()`, `rand::random()` in domain/workflow code | `&dyn Clock` / `&dyn IdGen` injected, or the value passed as a parameter |
| `sqlx::Error` / `reqwest::Error` in a workflow or handler signature | Translated to a domain error in the repository — the anti-corruption layer |
| `let mut v = vec![]; for x in xs { v.push(f(x)?) }` | `xs.iter().map(f).collect::<Result<Vec<_>, _>>()?` — the traverse |
| Recursion for iteration | Iterators, `fold`, or an explicit worklist. Rust has no guaranteed TCO |
| `async fn` in `Drop`, spawning in `Drop` | An explicit compensation stack (`references/sagas.md`) |
| `Arc<Mutex<AppState>>` threaded everywhere | Own the state in a task; talk to it over a channel |
| Unbounded concurrency | `buffer_unordered(n)`, bounded channels, `JoinSet` with a cap |
| `mockall` in the default test path | Hand-written fake `impl`s of your own traits |
| `im` (archived), `fp-core`, `higher` (abandoned) | `imbl`, or `Arc::make_mut` / `Cow` first. No HKT emulation |
| `unsafe` | `#![forbid(unsafe_code)]`, or a written justification in the crate docs |

## Concept → Rust realization

| FP concept | Rust realization |
|---|---|
| Two-track `Result` / Railway Oriented Programming | `Result<T, E>` + `?` as the track switch + `thiserror` enums; `From` impls do the track conversion |
| Designing with types | Newtypes with private fields (`nutype` 0.7), enums for every "or", typestates via distinct structs or `PhantomData<Marker>` |
| Parse, don't validate | `#[serde(try_from = "RawX")]` on domain types, `TryFrom<Row>` for DB rows, clap `value_parser` for argv, typed `Config` from env at startup |
| Functional core / imperative shell | Crate per layer — `server → api → app → domain`, `infra` implements `app`'s traits. The direction is enforced by `Cargo.toml`, not by review |
| Traverse / sequence | `.collect::<Result<Vec<_>, E>>()` — `Result` implements `FromIterator` and short-circuits on the first `Err` |
| Applicative validation (collect *all* errors) | `itertools::partition_map` / `process_results`, or `validated` / `frunk::Validated`; `garde` for form field accumulation |
| Reader monad / dependency injection | Trait ports + a context struct, generic (`impl Repo`) or dynamic (`Arc<dyn Repo>`). **Honestly: there is no Reader monad in Rust** — no `R` type parameter threading, no `Layer`. Plain generics and one wiring site in `main.rs` |
| Bracket / resource safety / rollback | RAII: `Drop` on `sqlx::Transaction` queues `ROLLBACK`, and `?`'s early exit drops it. Ownership does the cleanup — no `finally`, no leaked handle |
| Commands in, events out | Workflows take a parsed command type, return domain events; the shell dispatches them |
| Property-based testing | `proptest` 1.11 for the pure core, `proptest-state-machine` 0.8 for model-based lifecycle tests, `cargo-mutants` to prove the error track is tested |
| Immutable persistent collections | `imbl` 7 — but reach for `Cow` / `Arc::make_mut` first; ownership already gives you most of what persistence was for |
| Two-track early exit inside a fold | `std::ops::ControlFlow` |

## What doesn't map

A port that admits what doesn't transfer is more trustworthy than one that pretends parity. `SKILL.md` tells the agent these are dead ends so it does not burn a session emulating them badly:

| Missing | Status | What to do instead |
|---|---|---|
| **Higher-kinded types** | Not in the language | You cannot write a `Monad`/`Functor` trait abstracting over `Result`, `Option`, and `Future`. Write concrete code per type. Do **not** pull in `fp-core` or `higher` — both abandoned |
| **Custom `?`** | `try_trait_v2` is nightly-only, tracking issue flagged with design concerns | `Result` and `Option` are the only railway types with syntax support. Design around them; do not invent your own |
| **Effect polymorphism** | Keyword-generics / Effects Initiative is **dormant** | Sync and async are separate colours; you will sometimes write a function twice. You get `E` free via `Result`, but requirements are plain generics or trait objects, and there is no `Layer` |
| **Guaranteed tail calls** | `become` is nightly | Deep recursion overflows the stack. Use iterators, or an explicit worklist for tree walks |
| **Async `Drop`** | `AsyncDrop` is nightly | RAII cleanup that must `await` (releasing a distributed lock, compensating a remote call) does not work. Use an explicit compensation stack — `references/sagas.md` |

**What you get in exchange:** ownership makes aliasing bugs impossible without needing immutability; enums and exhaustiveness are checked by the compiler rather than a linter; cargo enforces architecture boundaries at resolution time; and zero-cost abstractions mean newtypes and typestates compile to nothing at all.

## What's inside

```
rust-fp-skill/
├── SKILL.md                     # entry point: philosophy, hard rules, decision table,
│                                # anti-patterns, 10-step workflow, reference index,
│                                # and what does NOT map from other FP languages
└── references/
    ├── scaffold.md              # toolchain pin, crate-per-layer workspace, [workspace.lints]
    │                            # deny tier, clippy.toml disallowed-methods, rustfmt/deny.toml, CI
    ├── domain-types.md          # newtypes & nutype, algebraic data types, typestates with
    │                            # PhantomData, non-empty collections, money, time
    ├── rop-errors.md            # thiserror enums, per-layer taxonomy, expected-error vs defect,
    │                            # error-crate decision matrix, traverse, accumulation, retry
    ├── boundaries.md            # #[serde(try_from)], the four boundaries, sum types over the
    │                            # wire, round-trip as the contract
    ├── pattern-matching.md      # exhaustiveness, #[non_exhaustive] trade-off, let-else,
    │                            # let-chains (edition 2024), ControlFlow, total state machines
    ├── di-context.md            # dependency-inversion arrow, generics vs dyn, AFIT vs
    │                            # async-trait, Clock/IdGen/Rng, config, the single wiring site
    ├── database.md              # sqlx repositories, anti-corruption layer, rich enums in SQL,
    │                            # transaction threading, atomic conditional updates, N+1, migrations
    ├── concurrency.md           # bounded concurrency, JoinSet, cancellation safety, channels
    │                            # and actors, graceful shutdown
    ├── sagas.md                 # compensating actions, why async Drop doesn't work, explicit
    │                            # compensation stacks, when to stop rolling your own
    ├── testing.md               # the pyramid, proptest patterns, proptest-state-machine,
    │                            # testing the error track, fakes over mocks, determinism, tooling
    ├── production.md            # #[instrument(err)], OpenTelemetry, tower resilience layers,
    │                            # error→HTTP mapping, health vs readiness, containers, cargo-deny
    ├── app-shapes.md            # HTTP API (axum), CLI (clap), library crate, WASM/full-stack
    └── code-review.md           # MANDATORY self-review: mechanical greps, dependency-direction
                                 # audit, error-channel/type-design/runtime/test audits
```

Every reference ends in a checklist. All API examples were verified against the versions in [Version policy](#version-policy).

## Proof repo

**[github.com/mikezupper/rust-fp-skill-examples](https://github.com/mikezupper/rust-fp-skill-examples)**

A complete commerce API — auth, catalog with a **total** category-tree builder, cart, atomic checkout, order history — built strictly under this skill's rules:

| | |
|---|---|
| Architecture | 4-crate workspace: `domain` (leaf, no async/no I/O) → `app` (workflows + port traits) → `infra` (adapters) → `api` (axum router) |
| Stack | axum 0.8 + sqlx 0.9 / SQLite, tokio, tracing, thiserror, nutype, proptest |
| Tests | **58 passing**, including property tests over the pure core and an explicit checkout-atomicity proof against a real transaction |
| Lints | Clean under `cargo clippy -- -D warnings` with the full deny tier — `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `wildcard_enum_match_arm` |

The category-tree builder is the instructive one. It is a **total** function from a flat row set to a forest: every input category appears in the output exactly once, for *every* input — a self-parent, a dangling parent, or an `a → b → a` cycle all produce roots rather than an infinite loop or, worse, silently dropped rows. The obvious implementation loses cycle participants; it passes every example test and fails the totality property in seconds. That property test is in the repo.

Bugs found while building this app were fed back into the reference files — the references are what the example actually needed, not what sounded good in the abstract.

## Installation

A skill is just a folder; installation is a copy.

**Global (all projects):**

```bash
git clone https://github.com/mikezupper/rust-fp-skill ~/.claude/skills/rust-fp-skill
rm -rf ~/.claude/skills/rust-fp-skill/.git   # keep the skill folder plain content
```

**Per-project:**

```bash
git clone https://github.com/mikezupper/rust-fp-skill <your-project>/.claude/skills/rust-fp-skill
rm -rf <your-project>/.claude/skills/rust-fp-skill/.git
```

Already have the repo checked out? A plain copy works the same: `cp -r rust-fp-skill ~/.claude/skills/rust-fp-skill` (minus the `.git`).

Then start a new Claude Code session. The `description` frontmatter in `SKILL.md` makes the skill trigger automatically whenever the agent creates or modifies a Rust app, service, CLI, library, or workspace; you can also invoke it explicitly with `/rust-fp-skill`. To update later, re-clone (or `git pull` in your checkout and re-copy).

## How the skill works

**Progressive disclosure** — which is why it is a folder rather than one large file:

1. **`SKILL.md` loads when the skill triggers.** It carries the philosophy, the hard rules, a decision table ("situation → reach for"), the anti-pattern list, the 10-step build workflow, and the honest list of what does not map from other FP languages.
2. **Each workflow step points at exactly one reference file**, read only when the agent is working in that area — modeling the domain loads `domain-types.md`, designing errors loads `rop-errors.md`, touching persistence loads `database.md`, writing a saga loads `sagas.md`. Deep guidance without diluting the context window on areas the task never reaches.
3. **The final step is a mandatory self-review** (`code-review.md`): mechanical `rg` sweeps for banned constructs (including ones hiding behind an `#[allow]`), a dependency-direction audit, then error-channel, type-design, runtime/resource, and test audits, then a sweep of every checklist touched during the task — before the agent may declare the work done.

## How to best leverage it

**Prompting**

- **Just ask for the app.** "Build an inventory service with Postgres" is enough — the skill supplies the architecture. You do not need to say "use FP" or "no unwrap".
- **Say "production-ready"** when you mean it. That pulls the full `production.md` checklist — spans, `#[instrument(err)]`, tower timeout/retry/concurrency layers, graceful shutdown, health vs readiness, `cargo-deny` — into scope as acceptance criteria rather than suggestions.
- **Name the app shape** when it is ambiguous ("as a CLI", "as a library crate", "with a WASM frontend") so the right `app-shapes.md` section drives the scaffold.
- **Mention the database up front.** The transaction boundary is an architectural decision — this skill places it in the workflow layer, never in repositories — and it is much cheaper to get right in the first file than to retrofit.

**Reviewing the output**

- The agent runs the `code-review.md` pass itself, but that file is also an excellent *human* review script. Run the greps yourself in CI.
- Pair with an independent adversarial review pass (`/code-review`). This skill biases toward construction; a reviewer should bias toward destruction.
- Adopt `references/scaffold.md`'s `[workspace.lints]` and `clippy.toml` in your own repos, so the hard rules keep holding for code written without the skill.

**Customizing**

- **The rules are opinions — edit them.** Relax a lint tier, swap sqlx guidance for diesel or sea-orm, add house naming conventions, drop the crates you do not use.
- **Keep the format.** Hard rules + decision tables + anti-pattern lists + per-area checklists is the shape agents follow most reliably. Prose gets skimmed; tables get applied.
- Add project-specific rules as a *project* skill in `<project>/.claude/skills/` that references this one, rather than forking it per repo.

**Maintaining**

- Re-verify APIs on ecosystem moves — a sqlx major, an axum major, thiserror 3, a nutype release. The version notes at the top of each reference file mark exactly what to check.
- When you fix a bug the skill's rules should have prevented, add the rule. That is how the reference files got the content they have.

## Version policy

- **Target: Rust 1.88+ on edition 2024.** Edition 2024 is **required**, not preferred — let-chains are stable since 1.88 but edition-gated and error on 2021, and the 2024 capture/lifetime rules are assumed throughout the references. Current stable at time of writing is **1.97.1**; the scaffold pins the toolchain in `rust-toolchain.toml`.
- **API examples verified 2026-07** against: `thiserror` 2.0, `anyhow` 1.0 (binaries only), `axum` 0.8, `tower`/`tower-http`, `sqlx` 0.9, `tokio` 1.53, `tokio-util` 0.7, `tracing` 0.1.44, `proptest` 1.11 with `proptest-state-machine` 0.8, `nutype` 0.7, `itertools` 0.15, `rust_decimal` 1.42, `imbl` 7, `async-trait` 0.1, `trait-variant` 0.1.
- **`sqlx`'s repository moved to `transact-rs/sqlx`.** Old launchbadge links are dead; update bookmarks and any vendored docs.
- **`sea-orm` 2.0 is very new** (July 2026) — expect churn. The references default to `sqlx` for that reason; use sea-orm deliberately, not by accident.
- **`jiff` is still pre-1.0.** `chrono` remains the safe default in a public API surface; flag `jiff` if it appears in one.
- Nightly-only features (`try_trait_v2`, `become`, `AsyncDrop`, keyword generics) are documented as *unavailable*, not as roadmap. If any of them stabilize, the [What doesn't map](#what-doesnt-map) table is the first thing to revisit.

## Sources & credits

- **Scott Wlaschin — [F# for Fun and Profit](https://fsharpforfunandprofit.com)**: [Railway Oriented Programming](https://fsharpforfunandprofit.com/rop/) · [Designing with Types](https://fsharpforfunandprofit.com/series/designing-with-types/) · [Property-Based Testing](https://fsharpforfunandprofit.com/series/property-based-testing/) · [A Recipe for a Functional App](https://fsharpforfunandprofit.com/series/a-recipe-for-a-functional-app/) · [FP Patterns](https://fsharpforfunandprofit.com/fppatterns/) · [Thinking Functionally](https://fsharpforfunandprofit.com/series/thinking-functionally/)
- **Alexis King — [Parse, don't validate](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/)** (2019), the boundary discipline this skill builds `#[serde(try_from)]` around
- **Hoverbear — [Pretty State Machine Patterns in Rust](https://hoverbear.org/blog/rust-state-machine-pattern/)**, the typestate lineage
- **[Rust Design Patterns](https://rust-unofficial.github.io/patterns/)** ([rust-unofficial/patterns](https://github.com/rust-unofficial/patterns)) — the newtype idiom and RAII guards
- **[Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)** ([rust-lang/api-guidelines](https://github.com/rust-lang/api-guidelines)) — naming, error types, and public-surface hygiene
- **[The Rust Programming Language](https://doc.rust-lang.org/book/)** and the [Rust reference](https://doc.rust-lang.org/reference/) — the stabilization facts behind the version policy
- **[Claude Code skills](https://code.claude.com/docs/en/skills)** — the skill format and the progressive-disclosure model

*This skill encodes one opinionated synthesis of the above. None of the cited authors, projects, or maintainers endorse it, and any mistakes in the mapping are mine.*

## License

Text and markup licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) © 2026 Mike Zupper.
