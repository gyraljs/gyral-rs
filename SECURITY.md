# Security policy

## Supported code

Security fixes currently target the latest `main` revision. The Rust workspace
is pre-1.0, has no published crates.io release, and has no maintained release
branches. Use a reviewed revision and include its commit hash in a report.
Applications remain responsible for their deployment and data policies.

## Report a vulnerability privately

Use [GitHub's private vulnerability reporting](https://github.com/gyraljs/gyral-rs/security/advisories/new)
for this repository: **Security → Report a vulnerability**. If that channel is
unavailable, contact the Gyral maintainers at [info@gyral.dev](mailto:info@gyral.dev).
Do not post a vulnerability or sensitive reproduction in a public issue.

Include the affected revision and package, target and toolchain, Gyral and V8
versions, impact, and a minimal reproduction using synthetic data. Describe any
relevant bundle, runtime-limit, HTTP, or container configuration. Keep credentials,
private DTOs, cookies, and personal content out of the report when possible.

Maintainers assess reports against the current code and coordinate fixes and
disclosure with the reporter. This experimental project makes no fixed response
time or long-term-support commitment.

## Understand the runtime boundary

The embedded renderer executes trusted application bundles in the host process.
JavaScript API restrictions, deadlines, fresh isolates, and heap budgets do not
provide a process sandbox or a process-memory cap. Arbitrary uploaded scripts,
customer plugins, and downloaded third-party code are outside the supported
scope. Fatal native failures and operating-system OOM can affect the server.

The Askama example executes no JavaScript on the server. Its anonymous notebook
cookie, CSRF protection, and local HTTP settings demonstrate application
boundaries; they do not provide production identity or recovery.

Read [the security design](docs/security.md) for DTO and telemetry handling,
artifact integrity, CSP, example limits, and the narrowly scoped upstream
dependency exception. The [operating guide](docs/design/operations-and-release.md)
describes native archive verification and deployment responsibilities.
