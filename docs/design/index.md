# Design decisions

The owner selected a new Rust repository, `deno_core` for embedded rendering,
the Rust FP and Sense of Style skills, Beads for task tracking, and the supplied
OpenAI harness reference for repository organization.

The owner later authorized the [snapshot and worker evaluation](runtime-evaluation.md)
and replacement of local Gyral tarballs with the published npm release `0.3.1`.
The runtime experiment lives in the test harness; application-facing startup
continues to create a fresh isolate and evaluate its bundle for each request.

The scaffold uses these defaults:

| Decision | Reason |
| --- | --- |
| A `gyral` front door over two application-facing crates and one domain crate | Give applications one dependency while keeping the HTTP adapter small and the core pure through Cargo dependencies. |
| Trusted app bundles supplied by each application | Execute the app's existing views while leaving authorization and data loading in Rust. |
| One embedded worker and a fresh isolate per request | Establish request isolation before optimizing repeated rendering. |
| Configurable deadlines, a managed-heap budget, and a dedicated watchdog | Interrupt JavaScript faults while preserving a fresh isolate for the next request. |
| Complete HTML strings initially | Keep the Rust/JavaScript boundary small and testable; streaming can follow a demonstrated need. |
| Both lockfiles, exact example pins, and caret library requirements | Reproduce the tested graph here without forcing consumers into resolution conflicts; `deno_core` stays exact. |
| SSR protocol 1 and a verified server/browser manifest | Accept any stable Gyral release of a supported protocol, so Gyral patches need no gyral-rs release; reject unknown protocols and mixed files; use one digest for browser URLs, SRI, and CSP. |
| MIT license and crates.io publication | Match Gyral's license; publish the four crates together through a reviewed [release workflow](../releasing.md). |

The owner asked what untrusted bundles mean. The initial scope remains reviewed
code shipped with an application. Uploaded plugins or customer-supplied
JavaScript would need stronger process isolation and a separate design.

Read [operations and release](operations-and-release.md), the [bundle contract](bundle-contract.md), [assets and CSP](assets-and-csp.md), [runtime limits](runtime-limits.md), [architecture](../../ARCHITECTURE.md),
and [security boundary](../security.md). Beads holds implementation work and
dependencies; this document records decisions rather than task status.

The owner also requested [Askama pages with client-only Gyral](askama-client.md).
That example uses Rust templates for the document and browser rendering for
component contents. It has its own client asset manifest and no embedded runtime
dependency. The existing embedded SSR contract remains a separate rendering path.

The owner requested a multi-page forms application and approved implementation
of the notes example. Its selected defaults are anonymous notebooks per browser,
native POST/303 flows, SQLite persistence, Rust ownership and CSRF checks, and a
Gyral preview observing light-DOM controls. The application domain and workflows
live in separate unpublished example crates. Account sign-in, client routing,
fetch-based saves, and production operation are outside this example's scope.
