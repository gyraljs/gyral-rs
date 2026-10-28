# Security boundary

The renderer executes JavaScript reviewed and shipped by the host application's
developer. Incoming props can be untrusted data; Rust parses their shape and
limits their serialized size, while Gyral escapes interpolated content. Code and
data are separate inputs. The bridge never constructs executable JavaScript by
inserting raw prop values.

Uploaded plugins, customer JavaScript, and arbitrary downloaded code are outside
the supported scope. An embedded engine shares the Rust process. Restricting
JavaScript APIs does not provide a process sandbox or prevent resource exhaustion.

The default runtime exposes no application filesystem or network API. The host
authorizes requests and passes only the DTO a browser may receive. Server cookies,
database credentials, and private authentication material must stay out of props
and hydration seeds. Renderer debug output omits props, HTML, and bundle source.
JavaScript diagnostics can still include private values, so the HTTP adapter logs
only its status and finite error category and returns a fixed error message.

The renderer bounds bundle size, props size, returned HTML size, and queue length.
It now applies [render deadlines, a managed V8 heap budget, cancellation, and
bounded shutdown](design/runtime-limits.md). Runaway JavaScript is interrupted;
each subsequent request uses a fresh isolate. Oversized V8 output is rejected
before an unbounded Rust string copy, then checked against the UTF-8 byte limit.

These controls are not a process-memory cap or an untrusted-code sandbox.
The heap callback allows temporary termination headroom; external buffers and
native allocations remain outside the heap budget. A caller deadline cannot
force native engine initialization or teardown to finish.
Fatal V8 errors, operating-system OOM, or a native engine stall can still affect
the host process. A timeout result does not prove native cleanup has completed.
Do not use this package to execute arbitrary third-party code.

Startup now verifies the [artifact manifest](design/bundle-contract.md) against
server and browser bytes. The example serves immutable verified bytes and applies
a [hash-based document CSP](design/assets-and-csp.md); browser checks enforce it
and reject a substituted client file. Hashes require a trusted manifest and build;
they are not signatures. The helper covers static styles and one browser entry.

The [operating example](design/operations-and-release.md) verifies request
telemetry, readiness, signal shutdown, a sustained fixture profile, and pinned
native release inputs. Before deploying an application, establish its process
resource policy, CSP, identity, artifact retention, and workload-specific evidence.
For untrusted code, design process isolation separately rather than extending this
contract with a flag. Follow-up implementation is tracked in Beads.

## Client-only notes example

The [Askama notes application](design/askama-client.md) executes no server-side
JavaScript. Rust authorizes every database read and write with an anonymous
notebook capability from an HttpOnly, SameSite=Strict cookie. Ownership is part
of the SQL condition, including atomic update/delete operations. The cookie has
a 30-day lifetime and is the access credential; there is no account recovery.
Clearing it starts a different notebook without deleting old records.

Native forms carry a session-bound HMAC-SHA256 CSRF proof. Rust checks it in
constant time before writes. The startup-generated signing key rotates on
restart, preserving notes while expiring old forms. Failed field validation,
expired proofs, and quota failures preserve drafts without mutation. Strict form
decoding, a 64 KiB body limit, validated fields, and a 200-note notebook quota
bound individual requests and views. Total notebooks and database size are not
capped. Logs omit cookie/proof values, note content, and SQL driver diagnostics.

The Gyral preview receives approved title/body text and observes native controls;
it has no authority to save or load data. CSP permits native same-origin form
submission and denies component network access. Browser checks prove the full
flow without JavaScript and reject injection-shaped content.

The example binds to loopback by default and uses synthetic data over HTTP.
Its cookie does not use Secure. Applications that deploy this pattern own
authenticated identities, HTTPS cookies, recovery, retention, and storage
capacity policies. The example demonstrates those boundaries without providing
a production identity service.

The dependency gate has one scoped maintenance exception:
[RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436.html) marks
`paste` as unmaintained. V8 150.4.0 uses its compile-time macro; this advisory does
not report a runtime vulnerability. `deny.toml` permits that specific advisory;
the lockfile pins the dependency graph. Beads issue `gyral-rs-83t` tracks removal when upstream
replaces the dependency. All other advisory checks remain enabled.

The OTLP dependency graph includes target-conditional `webpki-root-certs` 1.0.9 certificate data
under [CDLA-Permissive-2.0](https://cdla.dev/permissive-2-0/). A version-specific
license allowance covers that data; release assembly includes its agreement.
This is separate from the existing `paste` maintenance exception.
