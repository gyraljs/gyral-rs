# Third-party notices

The project uses MIT-licensed Gyral npm packages and `deno_core`, which embeds the
V8 engine through its Rust dependencies. Upstream packages retain their own
licenses and notices. `Cargo.lock`, `pnpm-lock.yaml` registry integrity hashes, and
`cargo deny` identify and check the dependency graph; this document is not a
replacement for their license texts.

The pinned Rust FP, Sense of Style, and Beads skill copies include their supplied
licenses and notices where available. Their source commits and file hashes are
recorded in [skills.lock.json](skills.lock.json). The writing skill is an
independent synthesis; Steven Pinker's copyrighted book is not included.

The [harness reference](docs/references/openai-harness-engineer.md) was supplied
by the owner. It preserves Ryan Lopopolo's attribution and OpenAI source URL.

The [Code of Conduct](CODE_OF_CONDUCT.md) follows the
[Gyral organization's copy](https://github.com/gyraljs/gyral/blob/main/CODE_OF_CONDUCT.md)
of Contributor Covenant 2.1. Its upstream attribution, enforcement guidelines,
and public community reporting contact are preserved.

Local binary release artifacts collect available Cargo dependency license/notice
texts. The target-conditional OTLP TLS certificate-data package `webpki-root-certs` 1.0.9 uses
[CDLA-Permissive-2.0](https://cdla.dev/permissive-2-0/); its agreement is included
in the archive. The [release inventories](docs/design/operations-and-release.md)
record Rust dependencies, the pinned native V8 archive, and Gyral inputs. They
do not inventory the full container operating system or all JavaScript tooling.
