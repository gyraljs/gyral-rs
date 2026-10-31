# Releasing the crates

This runbook is for the repository owner. It publishes four crates to
[crates.io](https://crates.io), always together and always in this order:

| Order | Crate | Directory | Depends on |
| --- | --- | --- | --- |
| 1 | `gyral-renderer-domain` | `crates/domain` | — |
| 2 | `gyral-renderer` | `crates/gyral-renderer` | domain |
| 3 | `gyral-axum` | `crates/gyral-axum` | renderer |
| 4 | `gyral` | `crates/gyral` | renderer, optional axum |

`gyral` is the front door that applications add first; the other three are its
building blocks. Examples set `publish = false` and never reach the registry.

The [release workflow](../.github/workflows/release.yml) runs only when started by
hand from `main`. Its `verify` job runs the complete gate, the package checks,
cargo-semver-checks, and the release evidence build. cargo-semver-checks skips a
crate until its latest published version is 0.1.0 or later, so the first release
checks none of the four (see [the `gyral` placeholder](#the-gyral-placeholder)). Its `publish` job waits for
approval in the `crates-io` GitHub environment, publishes every crate whose
version is not yet on crates.io, tags `v<version>`, and creates a GitHub release
with the evidence archives, SBOMs, and `SHA256SUMS`. A rerun skips crates that are
already published and leaves an existing GitHub release unchanged.

crates.io can authorize the workflow through
[Trusted Publishing](https://crates.io/docs/trusted-publishing), which exchanges a
GitHub OIDC token for a 30-minute crates.io token. As of October 2026, crates.io
documents that a crate must already exist before Trusted Publishing can be
configured for it. `gyral` already exists as a 0.0.1 placeholder, but the other
three do not, so the first release publishes all four with one API token; every
later release uses Trusted Publishing and no stored secret.

## One-time setup

### Accounts and names

1. Sign in at [crates.io](https://crates.io) with the GitHub account that will own
   the crates. crates.io has no password or two-factor setting of its own; it
   trusts the GitHub login. Enable
   [two-factor authentication on GitHub](https://github.com/settings/security),
   and require it for members of the `gyraljs` organization.
2. Add and verify an email address in your
   [crates.io profile](https://crates.io/settings/profile). crates.io refuses
   uploads from an account without a verified email.
3. Understand the namespace. crates.io has no organizations or reserved prefixes:
   anyone may publish an unrelated `gyral-something` crate, and only the names you
   publish are yours. Shared ownership comes from adding a GitHub team as an owner
   (below), not from the prefix. The owner already holds the bare `gyral` name:
   see [the `gyral` placeholder](#the-gyral-placeholder).

### The `gyral` placeholder

The owner published `gyral` 0.0.1 to hold the name. It has no API, and 0.1.0
supersedes it as the front door. Two consequences follow:

- **Trusted Publishing can be configured for `gyral` now**, with the values in
  [After the first publish](#after-the-first-publish). Leave **Require trusted
  publishing for all new versions** off until 0.1.0 is on crates.io: both paths
  below publish all four crates with the first-publish token, and that setting
  would make crates.io reject the token upload of `gyral` 0.1.0.
- **cargo-semver-checks does not compare 0.1.0 with 0.0.1.** `tools/release.py
  semver` skips a crate whose published versions are all below 0.1.0, since a
  placeholder has no API to keep compatible. From the next release on, `gyral`
  is checked against 0.1.0 like the other three crates.

### GitHub environment and tag ruleset

1. In the repository's **Settings → Environments**, create an environment named
   exactly `crates-io`.
   - **Required reviewers:** add yourself and, ideally, a second maintainer.
     Enable **Prevent self-review** only if someone else can approve.
   - **Deployment branches and tags:** choose **Selected branches and tags** and
     allow only `main`.
2. In **Settings → Rules → Rulesets**, add a **tag ruleset** named
   `release tags`, enforcement **Active**, targeting the pattern `v*`. Enable
   **Restrict updates**, **Restrict deletions**, and **Block force pushes**, so a
   published tag can never move. Leave **Restrict creations** off: the workflow
   creates the tag with its `GITHUB_TOKEN`. If you also restrict creations, first
   confirm that GitHub lets you add the GitHub Actions app as a bypass actor.

### A short-lived token for the first publish

Create the token at [crates.io → Account Settings → API Tokens → New Token](https://crates.io/settings/tokens/new):

- **Name:** `gyral-rs first publish`
- **Expiration:** 7 days, or a custom date one or two days away
- **Scopes:** `publish-new` and `publish-update` only
- **Crates:** four exact patterns, `gyral`, `gyral-renderer`,
  `gyral-renderer-domain`, and `gyral-axum`; avoid a `gyral*` wildcard.
  `gyral` needs `publish-update` because its placeholder exists; the other three
  need `publish-new`.

crates.io shows the token once. Keep it in a password manager, not a file in the
checkout, and revoke it as soon as the first release is finished.

## First release (0.1.0)

Start from the merged release pull request: `CHANGELOG.md` has a dated
`## [0.1.0] - YYYY-MM-DD` heading, and CI passed on that commit on `main`. Then
choose one of two ways to publish. Path A follows the usual crates.io practice
of a local first publish. Path B builds the uploaded packages in CI.

### Path A: publish from your machine (recommended)

Use a fresh clone so no local edits reach the packages. The toolchain file
selects Rust 1.99.1.

```sh
git clone https://github.com/gyraljs/gyral-rs.git gyral-rs-release
cd gyral-rs-release
git checkout <merged release commit>
make prepare-native        # verify the pinned V8 archive
make package               # dry-run all four crates, check contents, docs.rs build

read -rs CARGO_REGISTRY_TOKEN && export CARGO_REGISTRY_TOKEN   # paste the token

cargo publish --dry-run --locked -p gyral-renderer-domain
cargo publish --locked -p gyral-renderer-domain
cargo publish --dry-run --locked -p gyral-renderer
cargo publish --locked -p gyral-renderer
cargo publish --dry-run --locked -p gyral-axum
cargo publish --locked -p gyral-axum
cargo publish --dry-run --locked -p gyral
cargo publish --locked -p gyral

unset CARGO_REGISTRY_TOKEN
```

Each `cargo publish` waits until crates.io serves the new version before it
returns, so the next crate can resolve it. If a command fails for an external
reason, such as the network or the token, fix it and continue from that crate.
A published version cannot be replaced: if the fix needs a source change, bump
all four crates to the next patch version and start again.

Then run the release workflow once, as in step 5 of
[every later release](#every-later-release). It finds all four crates published,
skips authentication and upload, and still tags `v0.1.0` and attaches the
CI-built evidence to the GitHub release.

### Path B: let the workflow publish

1. Add the token as an **environment secret** of `crates-io` named
   `CRATES_IO_BOOTSTRAP_TOKEN`. An environment secret is available only after a
   reviewer approves the `publish` job.
2. Run the workflow, as in step 5 of [every later release](#every-later-release).
   The publish job warns that it is using the bootstrap token and publishes all
   four crates in order.
3. Delete the `CRATES_IO_BOOTSTRAP_TOKEN` secret immediately afterward.

### After the first publish

1. **Configure Trusted Publishing** for each of the four crates (skip `gyral` if
   you configured it for the placeholder). Open the crate on crates.io, choose
   **Settings → Trusted Publishing → Add**, select GitHub, and enter:

   | Field | Value |
   | --- | --- |
   | Repository owner | `gyraljs` |
   | Repository name | `gyral-rs` |
   | Workflow filename | `release.yml` |
   | Environment | `crates-io` |

2. On the same page, enable **Require trusted publishing for all new versions**
   for each crate, including `gyral`, now that 0.1.0 is published. crates.io then rejects API-token uploads for that crate.
3. **Revoke the first-publish token** in [API Tokens](https://crates.io/settings/tokens),
   and confirm `CRATES_IO_BOOTSTRAP_TOKEN` is gone from the environment.
4. **Share ownership.** Add a second individual owner as a fallback, and add a
   `gyraljs` GitHub team so maintainers can publish without holding your account.
   Team owners can publish and yank but cannot add or remove owners. Approve
   crates.io's OAuth application in the organization's third-party access settings
   if GitHub asks; crates.io needs it to read team membership. Use each crate's
   **Settings → Owners** page, or a separate short-lived token with only the
   `change-owners` scope:

   ```sh
   for crate in gyral-renderer-domain gyral-renderer gyral-axum gyral; do
     cargo owner --add github:gyraljs:<team> "$crate"
     cargo owner --add <second-maintainer-login> "$crate"
   done
   ```

   An individual owner must accept the emailed invitation. Revoke the
   `change-owners` token afterward.

## Every later release

1. **Changelog.** Add a `## [x.y.z] - YYYY-MM-DD` section to `CHANGELOG.md`. Before
   1.0, an incompatible public API or documented telemetry-label change increases
   the minor version; compatible fixes increase the patch version.
2. **Version bump, all four crates together.** Set `version` in
   `[workspace.package]` of the root `Cargo.toml`; every crate inherits it. Update
   the four internal requirements in `[workspace.dependencies]`
   (`gyral-renderer-domain`, `gyral-renderer`, `gyral-axum`, `gyral`) to the same
   `version = "x.y.z"`, then run `cargo update --workspace` to refresh
   `Cargo.lock`.
3. **Compatibility inputs.** The crates accept Gyral bundles by
   [SSR protocol](design/bundle-contract.md#supported-protocols), so a new Gyral
   release that keeps a supported protocol needs **no gyral-rs release**. Start the
   [`gyral-canary` workflow](../.github/workflows/gyral-canary.yml) by hand with its
   version to test it, and adopt it in the examples later, under the two-week rule,
   as an ordinary dependency update. A Gyral release that declares a **new protocol**
   needs a gyral-rs **minor** that supports it, released before that Gyral release:
   add the protocol and its minimum Gyral release to `SUPPORTED_PROTOCOLS`, with
   matching server and browser artifacts, a compatibility fixture, and canary runs
   of the protocol's minimum and newest releases. Dropping a protocol or raising a
   minimum is also a minor. A `deno_core` or V8 change needs a reviewed
   `tools/v8-archive.json` entry and the same version, size, and digest in the
   [renderer guide](../crates/gyral-renderer/README.md#pin-the-native-v8-archive),
   and must keep the host environment that each supported protocol promises.
4. **Local gate.** Run `make ci`, `make gyral-canary`, and `make package`, open the
   release pull request, and merge it after CI passes.
5. **Run the workflow.** Open **Actions → release →
   Run workflow**, keep the branch `main`, and start it. The `verify` job takes
   most of the time. cargo-semver-checks compares each crate with its latest
   published version and fails on an undeclared breaking change. When `verify`
   passes, review the run and approve the `crates-io` deployment.
6. **Verify the release.** Check each crate page on crates.io, the docs.rs build
   at `https://docs.rs/crate/<crate>/x.y.z/builds`, the `vx.y.z` tag, and the
   GitHub release assets. Then build a fresh consumer, following the
   [native archive guidance](../crates/gyral-renderer/README.md#pin-the-native-v8-archive):

   ```sh
   cargo new gyral-release-check && cd gyral-release-check
   cargo add gyral@x.y.z --features axum
   cargo add serde_json
   cargo build
   cargo tree -i v8 --edges normal
   ```

## Dependency requirements

Published requirements decide what consumers' resolvers may choose, so they
differ from the repository's own pins. `Cargo.lock` stays committed and fixes
every version for this repository's builds, CI, and containers.

- **Ordinary dependencies** use caret requirements whose lower bound is the
  verified version, such as `tokio = "1.53.2"`. Exact `=` pins would force
  consumers into resolution conflicts with any other crate that needs a newer
  compatible release. `make policy` rejects an exact requirement on a library
  dependency.
- **Internal crates** use `version = "x.y.z"` plus `path`, so the workspace builds
  from source and the registry copy names the matching release.
- **`deno_core` stays exact (`=0.412.0`).** It is a 0.x crate, so a caret would
  already stop at 0.412.x. The exact pin also excludes 0.412.x patch releases,
  which may move to a new `deno_v8` line and with it a different V8 major
  version, native archive, and heap and termination behavior that the renderer
  was not tested against.
- **`v8` itself is left to the application.** `deno_core` 0.412.0 accepts V8 150.x.
  The application's lockfile and archive pin select the exact build, so a V8
  security fix within 150.x does not need a gyral-rs release. The renderer guide
  tells applications to keep `v8` at the reviewed 150.4.0 until they review a new
  archive.
- Examples, dev-dependencies, and tools keep exact pins; they never reach
  consumers' dependency graphs.

## When something goes wrong

- **A run failed before approval.** Nothing was published. Fix the cause on `main`
  and start a new run.
- **Publishing stopped partway.** Rerun the workflow. It skips crates already on
  crates.io and publishes the rest in order.
- **A published version is broken.** crates.io never deletes or replaces a
  version. Yank it on each crate's version page, or with
  `cargo yank --version x.y.z <crate>`, then release a fixed patch. Yanking stops
  new lockfiles from selecting the version; existing lockfiles keep working.
- **Trusted Publishing is rejected.** Compare the crate's configuration with the
  table above. The workflow filename, the environment name, and the repository
  must match exactly, and the run must come from `main`.
