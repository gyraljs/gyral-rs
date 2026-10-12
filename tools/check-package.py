#!/usr/bin/env python3
"""Check the packaged library crates and build renderer docs the way docs.rs does.

`make package` first runs `cargo publish --dry-run --workspace --locked`, which builds
each package from its extracted contents. This script then writes the same `.crate`
files without rebuilding, bounds their contents, and documents the renderer offline.
"""
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
# Publication order: each crate depends only on crates before it.
CRATES = ('gyral-renderer-domain', 'gyral-renderer', 'gyral-axum', 'gyral')
GENERATED = {'Cargo.toml', 'Cargo.toml.orig', 'Cargo.lock', '.cargo_vcs_info.json', 'README.md', 'LICENSE'}
MAX_CRATE_BYTES = 128 * 1024
MAX_FILE_BYTES = 64 * 1024


def version():
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--format-version', '1', '--no-deps', '--locked'], cwd=ROOT, text=True))
    versions = {package['version'] for package in metadata['packages'] if package['name'] in CRATES}
    if len(versions) != 1:
        raise SystemExit(f'Published crates must share one version, found {sorted(versions)}')
    return versions.pop()


def check_contents(path, name):
    errors = []
    if path.stat().st_size > MAX_CRATE_BYTES:
        errors.append(f'{path.name}: {path.stat().st_size} bytes exceeds {MAX_CRATE_BYTES}')
    with tarfile.open(path) as archive:
        members = [member for member in archive.getmembers() if member.isfile()]
        for member in members:
            relative = PurePosixPath(member.name).relative_to(path.name.removesuffix('.crate'))
            allowed = str(relative) in GENERATED or (relative.parts[0] == 'src' and relative.suffix == '.rs')
            if not allowed:
                errors.append(f'{name}: unexpected packaged file {relative}')
            if member.size > MAX_FILE_BYTES:
                errors.append(f'{name}: {relative} is {member.size} bytes')
    print(f'{path.name}: {path.stat().st_size} bytes, {len(members)} files')
    return errors


def docs_rs_build(packages, scratch):
    """Document each extracted package without the repository's Cargo configuration or network."""
    for path in packages.values():
        with tarfile.open(path) as archive:
            archive.extractall(scratch, filter='data')
    current = version()
    directories = {name: scratch / f'{name}-{current}' for name in CRATES}
    environment = {key: value for key, value in os.environ.items() if not key.startswith('RUSTY_V8_')}
    environment.update({
        'DOCS_RS': '1',
        'CARGO_TARGET_DIR': str(ROOT / 'target/docs-rs'),
        'RUSTDOCFLAGS': '-D warnings',
        # The extracted packages sit outside the checkout and its toolchain file.
        'RUSTUP_TOOLCHAIN': tomllib.loads((ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel'],
    })
    # docs.rs builds have no network. Cargo runs offline after fetching, and any
    # build-script download through Python or curl would fail at this proxy.
    offline = {name: 'http://127.0.0.1:9' for name in ('http_proxy', 'https_proxy', 'HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY')}
    for index, (name, directory) in enumerate(directories.items()):
        # Earlier crates resolve to their extracted packages, as crates.io would serve them.
        patches = [argument for sibling in CRATES[:index]
                   for argument in ('--config', f'patch.crates-io.{sibling}.path="{directories[sibling]}"')]
        subprocess.run(['cargo', 'fetch', '--quiet', *patches], cwd=directory, env=environment, check=True)
        subprocess.run(['cargo', 'doc', '--offline', '--no-deps', '--lib', '--all-features', '--quiet', *patches],
                       cwd=directory, env=environment | offline, check=True)
    print(f'docs.rs simulation: documented {", ".join(CRATES)} with DOCS_RS=1, all features, no RUSTY_V8_ARCHIVE, offline.')


def main():
    current = version()
    subprocess.run(['cargo', 'package', '--locked', '--no-verify', '--quiet', *(f'--package={name}' for name in CRATES)], cwd=ROOT, check=True)
    packages = {name: ROOT / 'target/package' / f'{name}-{current}.crate' for name in CRATES}
    errors = []
    for name, path in packages.items():
        errors.extend(check_contents(path, name))
    if errors:
        raise SystemExit('\n'.join(errors))
    with tempfile.TemporaryDirectory(prefix='gyral-docs-rs-') as scratch:
        docs_rs_build(packages, Path(scratch))


if __name__ == '__main__':
    main()
