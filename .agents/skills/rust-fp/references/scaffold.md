# Project Scaffold — and the Mechanical Enforcement Layer

Verified against Rust 1.97.1 stable (2026-07). **Target 1.88+ / edition 2024.** Edition 2024 is mandatory: let-chains are edition-gated (stable 1.88, edition 2024 only) and the 2024 capture/lifetime rules are assumed throughout. Everything here exists so the SKILL.md hard rules **fail the build** rather than rely on discipline. A rule with no enforcer will rot.

## 1. Toolchain pin

```toml
# rust-toolchain.toml — committed; CI and every dev get the same compiler
[toolchain]
channel = "1.97.1"
components = ["rustfmt", "clippy", "rust-src"]
```

## 2. Workspace: crate per layer

The headline advantage over other languages: **cargo enforces the onion at compile time.** `domain` cannot call the database because the symbol does not exist in its dependency graph. Not a lint, not a grep — a resolution error.

```
Cargo.toml   clippy.toml   rustfmt.toml   deny.toml    # config lives at the workspace root
crates/
  domain/  # LEAF. newtypes, enums, typestates, pure functions. no async, no I/O.
  app/     # workflows + service *traits*. depends on domain only.
  infra/   # repositories, HTTP clients, clock/id adapters. impls app's traits.
  api/     # axum router + handlers   (or crates/cli/ with clap)
  server/  # the binary: main.rs. the ONLY wiring site.
```

Edges are one-way: `server → api → app → domain` and `server → infra → app → domain`. `app` never names `infra`; it names traits that `infra` implements.

```toml
# Cargo.toml (workspace root)
[workspace]
members = ["crates/*"]
resolver = "3"                       # edition 2024 resolver

[workspace.package]
edition = "2024"
rust-version = "1.88"

# Versions pinned once, here. Member crates write `foo.workspace = true`.
[workspace.dependencies]
domain = { path = "crates/domain" }
app = { path = "crates/app" }
thiserror = "2.0.19"
anyhow = "1.0.104"                   # binaries only
nutype = "0.7.0"
rust_decimal = "1.42.1"
itertools = "0.15.0"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1.53.1", features = ["rt-multi-thread", "macros", "signal"] }
tokio-util = "0.7.19"
futures = "0.3.33"
axum = "0.8.9"
sqlx = { version = "0.9.0", features = ["runtime-tokio", "postgres", "macros"] }
tracing = "0.1.44"
async-trait = "0.1.91"               # dyn traits only; AFIT is not dyn-compatible
proptest = "1.11.0"

[profile.release]
lto = "thin"
codegen-units = 1
overflow-checks = true  # NON-NEGOTIABLE with integer-minor-unit money: wrapping loses funds silently
# `panic` is deliberately NOT set here: the default is unwind, and that is what a server needs.
# `panic = "abort"` buys a smaller binary and no unwind tables, but it also makes `catch_unwind`
# — and therefore `tower_http::CatchPanicLayer` — inert, so one panicking request kills the
# process and every other in-flight request with it. Unwind is what lets a defect become a 500.
# Set `panic = "abort"` only for CLIs, jobs, and workers that restart fast. See production.md §6.
```

### Proving `domain` is a leaf

```toml
# crates/domain/Cargo.toml
[package]
name = "domain"
edition.workspace = true

[dependencies]
thiserror.workspace = true
nutype.workspace = true
rust_decimal.workspace = true
serde = { workspace = true, optional = true }   # boundary DTOs only, behind a feature

[dev-dependencies]
proptest.workspace = true                       # dev-deps never enter the shipped graph

[features]
default = ["serde"]

[lints]
workspace = true
```

No `tokio`, `sqlx`, `axum`, `reqwest`, `uuid`-with-`v4`, or `chrono` clock — IDs and timestamps arrive as parameters. `crates/app` adds only `domain`, `thiserror`, `futures`, `tracing`, and `async-trait`; still no driver, no web framework, no HTTP client.

**Purity forcing function:** make `domain` `no_std`-compatible (`#![no_std] extern crate alloc;`). You then physically cannot reach `std::time`, `std::fs`, or threads — every accidental impurity becomes a compile error instead of a review note.

## 3. `[workspace.lints]` — the deny tier

Stable since Rust 1.74. Declared once; inherited by every crate carrying `[lints] workspace = true`.

```toml
[workspace.lints.rust]
unsafe_code = "forbid"                        # forbid: cannot be re-allowed downstream
missing_debug_implementations = "warn"
unreachable_pub = "warn"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }  # negative priority: specific lints below win
# totality — no partial functions
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
unreachable = "deny"
indexing_slicing = "deny"                     # xs[i] panics; use .get(i).ok_or(..)?
# exhaustiveness — adding a variant must break the build
wildcard_enum_match_arm = "deny"
match_wildcard_for_single_variants = "deny"
# numerics — money is integer minor units or rust_decimal
float_arithmetic = "deny"
as_conversions = "deny"                       # `as` truncates external data; use TryFrom
cast_possible_truncation = "deny"
cast_sign_loss = "deny"
# API hygiene — every Result-returning pub fn documents its error track
missing_errors_doc = "deny"
missing_panics_doc = "deny"
must_use_candidate = "deny"
result_large_err = "warn"
```

### Relax narrowly — never globally

```toml
# crates/server/Cargo.toml — only the binary is allowed to give up
[lints.clippy]
expect_used = "allow"   # startup wiring only; every expect must state its invariant
panic = "allow"
```

```rust
// crates/infra/src/lib.rs — tests may unwrap; production code may not.
// `feature = "testing"` is included so the shared fakes in `app/src/testing.rs` (di-context.md,
// testing.md) compile under the same relaxation instead of needing inline allows.
#![cfg_attr(any(test, feature = "testing"),
            allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing))]

// The ONE place `Utc::now` is permitted: the clock adapter itself.
#[allow(clippy::disallowed_methods, reason = "this IS the clock adapter; the ban makes it the only caller")]
fn now(&self) -> Timestamp { Timestamp::from(chrono::Utc::now()) }
```

An inline `#[allow]` inside `domain/` or `app/` is a review failure — push the exception into the crate that owns the impurity.

## 4. `clippy.toml` — ban by path

The key file. `disallowed-methods` and `disallowed-types` ban full paths outright: the direct analogue of ESLint `no-restricted-syntax`, and what turns "inject your clock" from advice into a build failure. Clippy walks ancestor directories, so one file at the workspace root covers every crate.

```toml
# clippy.toml
disallowed-methods = [
  { path = "std::time::SystemTime::now",  reason = "inject a Clock; see di-context.md" },
  { path = "chrono::Utc::now",            reason = "inject a Clock" },
  { path = "chrono::Local::now",          reason = "inject a Clock — and never use local time" },
  { path = "rand::random",                reason = "inject an Rng" },
  { path = "uuid::Uuid::new_v4",          reason = "inject an IdGen so tests are deterministic" },
  { path = "std::option::Option::unwrap", reason = "match, or ok_or(..)? onto the error track" },
  { path = "std::result::Result::unwrap", reason = "propagate with ?" },
  { path = "std::option::Option::expect", reason = "expect is startup-wiring-only" },
  { path = "std::result::Result::expect", reason = "expect is startup-wiring-only" },
  { path = "std::process::exit",          reason = "return from main so Drop guards run" },
  { path = "futures::future::join_all",     reason = "unbounded fan-out; use buffer_unordered(n)" },
  { path = "futures::future::try_join_all", reason = "unbounded fan-out; buffer_unordered(n) + try_collect. Narrow #[allow] for a fixed, code-controlled set only" },
  { path = "tokio::sync::mpsc::unbounded_channel", reason = "unbounded queue = OOM; channel(N) is backpressure — concurrency.md §7" },
]
disallowed-types = [
  { path = "anyhow::Error",         reason = "named thiserror enums everywhere except main.rs" },
  { path = "std::time::SystemTime", reason = "domain time is a newtype produced by the Clock" },
  { path = "std::sync::Mutex",      reason = "prefer owning state in a task + channel" },
  # Optional but recommended in `domain`: deterministic iteration order makes proptest
  # counterexamples reproducible. Infra and tests may keep HashMap.
  { path = "std::collections::HashMap", reason = "BTreeMap in domain code: deterministic iteration", replacement = "std::collections::BTreeMap" },
]
```

## 5. `rustfmt.toml` and `deny.toml`

```toml
# rustfmt.toml
edition = "2024"
max_width = 100
use_field_init_shorthand = true
use_try_shorthand = true
imports_granularity = "Crate"        # nightly-only (`cargo +nightly fmt`); ignored on stable
group_imports = "StdExternalCrate"   # nightly-only
```

```toml
# deny.toml
[advisories]
version = 2
unmaintained = "all"     # an archived crate is a vulnerability with a long fuse
yanked = "deny"
[licenses]
version = 2
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib"]
[bans]
wildcards = "deny"
multiple-versions = "warn"
deny = [
  { crate = "im",      reason = "archived 2022; use imbl 7.0.1" },
  { crate = "fp-core", reason = "abandoned HKT emulation" },
  { crate = "higher",  reason = "abandoned HKT emulation" },
]
[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

## 6. Tooling

| Tool | Version | Use |
|---|---|---|
| `cargo-nextest` | 0.9.140 | test runner: per-test process isolation, real parallelism |
| `cargo-deny` | 0.20.2 | licenses, bans, advisories, source pinning |
| `cargo-audit` | 0.22.2 | RustSec advisories against `Cargo.lock` |
| `cargo-llvm-cov` | 0.8.7 | coverage (`--lcov` for CI upload) |
| `cargo-mutants` | 27.1.0 | proves error branches are genuinely tested |

## 7. CI — where the rules become non-negotiable

```yaml
# .github/workflows/ci.yml
name: ci
on: [push, pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with: { toolchain: "1.97.1", components: "rustfmt, clippy" }
      - uses: Swatinem/rust-cache@v2
      - uses: taiki-e/install-action@v2
        with: { tool: "cargo-nextest,cargo-deny" }
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets --all-features -- -D warnings
      # The architecture assertion. `--edges normal` ignores dev/build deps.
      - name: domain is a leaf
        run: |
          if cargo tree -p domain --edges normal --prefix none \
             | grep -Ei '^(tokio|sqlx|axum|reqwest|hyper|diesel|sea-orm)\b'; then
            echo "::error::domain gained an infrastructure dependency"; exit 1
          fi
      - run: cargo build -p domain --no-default-features
      - run: cargo nextest run --workspace --all-features
      - run: cargo test --workspace --doc
      - run: cargo deny check advisories bans licenses sources
```

## Checklist

- [ ] `rust-toolchain.toml` pins an exact stable channel; CI installs the same one
- [ ] `edition = "2024"` and `resolver = "3"` set at the workspace level
- [ ] One crate per layer; `server` is the only binary and the only wiring site
- [ ] `crates/domain/Cargo.toml` has no tokio / sqlx / axum / reqwest / uuid-v4 / clock
- [ ] `domain` compiles with `--no-default-features` (ideally `no_std`)
- [ ] `[workspace.lints]` carries the full deny tier plus `unsafe_code = "forbid"`
- [ ] Every member crate has `[lints] workspace = true`
- [ ] Relaxations are per-crate or `#[cfg_attr(test, allow(..))]` — never inline in domain/app
- [ ] `clippy.toml` bans `now()`, `random()`, `new_v4()`, `unwrap`, `expect`, `join_all`/`try_join_all`, `unbounded_channel` by path
- [ ] `overflow-checks = true` in the release profile
- [ ] Release profile leaves `panic` at the default unwind for servers (`CatchPanicLayer` needs it); `abort` only for CLIs/jobs, with a comment saying why — `production.md` §6
- [ ] `deny.toml` sets `unmaintained = "all"` and an explicit license allow-list
- [ ] CI runs fmt --check, clippy -D warnings, nextest, deny, and the `cargo tree -p domain` assertion
