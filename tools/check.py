#!/usr/bin/env python3
"""Check repository knowledge, pinned sources, and package boundaries."""
from pathlib import Path
import hashlib
import json
import re
import sys
import tomllib
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parent.parent
errors = []

pin = json.loads((ROOT / "tools/v8-archive.json").read_text())
lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
if [package["version"] for package in lock["package"] if package["name"] == "v8"] != [pin["version"]]:
    errors.append("Native V8 pin differs from Cargo.lock; review a new archive before upgrading")
configuration = tomllib.loads((ROOT / ".cargo/config.toml").read_text())["env"]["RUSTY_V8_ARCHIVE"]
if configuration != {"value": f'.local/v8/{pin["file"]}', "relative": True, "force": True}:
    errors.append("Cargo must consume the reviewed local native archive")
if pin["target"] != "x86_64-unknown-linux-gnu" or not re.fullmatch(r"[a-f0-9]{64}", pin["sha256"]):
    errors.append("Native archive target or digest is malformed")

for skill in json.loads((ROOT / "skills.lock.json").read_text())["skills"]:
    for name, expected in skill["files"].items():
        path = ROOT / name
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            errors.append(f"{name}: pinned skill changed; update its source and lock explicitly")

javascript = json.loads((ROOT / 'package.json').read_text())
javascript_lock = (ROOT / 'pnpm-lock.yaml').read_text()
for name in ('@gyral/core', '@gyral/ssr'):
    if javascript['dependencies'].get(name) != '0.3.4':
        errors.append(f'{name}: pin the verified npm release exactly')
    if not re.search(rf"  '{re.escape(name)}@0\.3\.4':\n    resolution: \{{integrity: sha512-[A-Za-z0-9+/]+=*\}}", javascript_lock):
        errors.append(f'{name}: registry SHA-512 integrity is missing from the lockfile')
if 'file:vendor/' in javascript_lock or 'file:vendor/' in json.dumps(javascript):
    errors.append('JavaScript dependencies still reference removed vendor archives')

documents = [ROOT / name for name in ("AGENTS.md", "README.md", "CHANGELOG.md", "ARCHITECTURE.md", "THIRD_PARTY_NOTICES.md", "CONTRIBUTING.md", "SECURITY.md", "CODE_OF_CONDUCT.md")]
documents.extend(sorted((ROOT / "docs").rglob("*.md")))
documents.extend(sorted((ROOT / ".github").rglob("*.md")))
documents.extend(sorted((ROOT / "crates").glob("*/README.md")))
links = 0
for document in documents:
    if not document.is_file():
        errors.append(f"{document.name}: required repository document is missing")
        continue
    text = document.read_text()
    for match in re.finditer(r"\[[^\]]*\]\(([^\s)]+)(?:\s+[^)]*)?\)", text):
        target = match.group(1).strip("<>")
        if target.startswith(("https:", "http:", "mailto:", "#")):
            continue
        path = (document.parent / unquote(target.split("#")[0])).resolve()
        links += 1
        if not path.exists():
            errors.append(f"{document.relative_to(ROOT)}: broken local link {target}")

allowed = {
    "domain": {"serde", "serde_json", "thiserror", "base64"},
    "gyral-renderer": {"gyral-renderer-domain", "deno_core", "serde_json", "thiserror", "tokio", "tracing", "sha2", "metrics"},
    "gyral-axum": {"gyral-renderer", "axum", "thiserror", "tracing"},
    "gyral": {"gyral-renderer", "gyral-axum"},
}
for directory, permitted in allowed.items():
    manifest = tomllib.loads((ROOT / "crates" / directory / "Cargo.toml").read_text())
    unexpected = set(manifest.get("dependencies", {})) - permitted
    if unexpected:
        errors.append(f"{directory}: forbidden dependency edge {sorted(unexpected)}; see ARCHITECTURE.md")
for source in (ROOT / "crates/domain/src").rglob("*.rs"):
    if re.search(r"\b(?:tokio|deno_core|axum|reqwest|sqlx)::|std::(?:fs|thread|net|env)::|std::time::(?:Instant|SystemTime)", source.read_text()):
        errors.append(f"{source.relative_to(ROOT)}: pure domain gained runtime or I/O code")

for directory, permitted in {
    "notes-domain": {"thiserror"},
    "notes-app": {"gyral-example-notes-domain", "thiserror", "async-trait", "tracing"},
}.items():
    manifest = tomllib.loads((ROOT / "examples" / directory / "Cargo.toml").read_text())
    unexpected = set(manifest.get("dependencies", {})) - permitted
    if unexpected:
        errors.append(f"{directory}: forbidden dependency edge {sorted(unexpected)}")
    for source in (ROOT / "examples" / directory / "src").rglob("*.rs"):
        if source.name == "tests.rs":
            continue
        code = source.read_text().split("#[cfg(test)]")[0]
        if re.search(r"\b(?:tokio|deno_core|axum|reqwest|sqlx)::|std::(?:fs|thread|net|env)::|std::time::(?:Instant|SystemTime)", code):
            errors.append(f"{source.relative_to(ROOT)}: domain/workflow gained runtime or I/O code")

client_example = tomllib.loads((ROOT / "examples/askama-client-app/Cargo.toml").read_text())
for forbidden in {"gyral", "gyral-renderer", "gyral-axum", "deno_core", "v8"}:
    if forbidden in client_example.get("dependencies", {}):
        errors.append(f"askama-client-app: forbidden embedded runtime dependency {forbidden}")

# Published libraries opt in with registry metadata; every other member stays private.
workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
license_text = (ROOT / "LICENSE").read_bytes()
published = {}
for member in workspace["workspace"]["members"]:
    package = tomllib.loads((ROOT / member / "Cargo.toml").read_text())["package"]
    if member.startswith("crates/"):
        published[package["name"]] = member
        if package.get("publish") is not True or not {"readme", "documentation", "keywords", "categories", "include"} <= package.keys():
            errors.append(f"{member}: published library needs publish = true and registry metadata")
        if (ROOT / member / "LICENSE").read_bytes() != license_text:
            errors.append(f"{member}/LICENSE must match the repository license")
    elif package.get("publish") is not False:
        errors.append(f"{member}: examples must set publish = false")
if set(published) != {"gyral", "gyral-renderer", "gyral-axum", "gyral-renderer-domain"}:
    errors.append(f"Unexpected published crate set {sorted(published)}; update docs/releasing.md")
# Exact requirements force conflicts on consumers. deno_core is the one reviewed exception.
library_dependencies = set()
for member in published.values():
    library_dependencies |= set(tomllib.loads((ROOT / member / "Cargo.toml").read_text()).get("dependencies", {}))
for name in sorted(library_dependencies - {"deno_core"}):
    requirement = workspace["workspace"]["dependencies"].get(name, {})
    version = requirement if isinstance(requirement, str) else requirement.get("version", "")
    if version.startswith(("=", "<", "~")) or not version:
        errors.append(f"{name}: published libraries need a caret requirement, found {version!r}")

if errors:
    print("\n".join(errors), file=sys.stderr)
    raise SystemExit(1)
print(f"Policy passed: 3 pinned skills, 2 pinned Gyral npm packages, {len(documents)} documents, {links} local links, crate boundaries, and {len(published)} publishable crates.")
