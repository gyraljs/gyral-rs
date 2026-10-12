# Quality gates

The repository makes important rules executable: domain purity through a separate
Cargo dependency graph, validated JSON boundaries, typed errors, forbidden unsafe
code and panic shortcuts, pinned skill hashes, registry lockfile integrity, and checked doc links.

Pure tests exercise boundary rejection, JSON round trips, and redacted debug output.
Runtime tests exercise real embedded JavaScript, promise rejection, invalid results,
request isolation, runtime capabilities, and timezone formatting. The real Gyral
fixture checks escaping and SSR metadata. Axum tests use a hand-written capability
and verify status, caching, content type, and diagnostic redaction. Browser smoke
tests verify initial HTML without JavaScript and interaction after hydration.

The separate Askama example checks a different contract: the response contains
custom-element tags and fallbacks, with no Gyral shadow content or seeds.
Browser checks cover first client rendering from string/number attributes,
delayed or unavailable JavaScript, ordinary form navigation, escaping, counter
interaction, keyboard focus, themes, narrow layout, and CSP/SRI enforcement.
Startup checks reject invalid client artifacts and verify that the executable's
normal dependency graph excludes the renderer and V8.

Notes checks verify native CRUD and 303 redirects with JavaScript enabled,
disabled, and blocked; 422 field errors preserve drafts and associated ARIA
messages. Rust tests reject missing/foreign CSRF proofs, malformed capabilities,
cross-notebook reads/writes, duplicate form fields, invalid IDs, and oversized
bodies. Real SQLite tests prove persistence after reopening, abandoned-transaction
rollback, and quota enforcement under concurrent creates. Workflow fakes cover
failures before and during commit. A schema check validates offline SQL metadata
against an empty database created from the migrations.

Browser checks prove that late preview registration preserves typed text,
disconnect/reconnect reads current controls, and shadow inputs do not contribute
to the outer form's FormData. They check escaping, enforcing CSP, narrow layout,
and 200% text scaling. The container test replaces the non-root container while
retaining its named volume and notebook cookie, then updates and deletes the
persisted note. Native and browser tests use temporary databases.

Manifest properties cover canonical digests, path binding, exact versions,
bounded metadata, and round trips through validated deserialization. Artifact
tests reject changed server/browser bytes; a startup smoke rejects five corrupt
or missing builds before HTTP binds. An unchanged build must reproduce its
manifest. Verified asset responses use immutable caching; misses and errors
remain uncached.

The expanded Gyral fixture checks seeded shared stores, form validation and
redaction, daylight-saving transitions, escaping, and recovery after real view
errors. Browser checks preserve server nodes and text entered before hydration,
exercise forms with and without JavaScript, and enforce the hash-based CSP.
Attempted inline code/styles and a substituted client file must be blocked.

Resource tests cover deadline interruption during initialization, synchronous
execution and microtasks; managed-heap pressure; cancellation; Rust unwind
recovery; owner drop; queue admission; and bounded shutdown. Recoverable failure
tests check a subsequent healthy request; stopped-worker tests assert unavailability.
Nextest isolates native tests in processes
and terminates a test after twenty seconds if interruption regresses. Native
watchdog tests use actual OS deadlines and condition-variable handshakes, as
explained in [engineering](engineering.md).

This evidence establishes bounded trusted rendering, not production readiness.
The release measurement establishes a sequential fixture baseline, without
pooling or snapshot comparisons. The [operating profile](design/operations-and-release.md) adds sustained synthetic
load budgets, overload recovery, readiness, safe telemetry, and signal shutdown.
It does not establish application latency/capacity, a process-memory cap, or a
complete Gyral feature matrix. The initial source-distribution and pre-1.0
compatibility policy is explicit; no published semver baseline exists until the
first [release](releasing.md), after which the release workflow compares each
crate's public API with it. Future work lives
in Beads; [verification](verification.md) records the commands actually run.
