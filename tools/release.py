#!/usr/bin/env python3
"""Release helpers for the crates.io workflow; this script never publishes anything.

    release.py version   Print the shared workspace version.
    release.py notes     Print the CHANGELOG.md section for that version; fail if absent.
    release.py pending   Print, in publication order, crates whose workspace version
                         is not yet on crates.io. Release reruns skip the others.
    release.py semver    Run cargo-semver-checks for each crate that has a published
                         baseline. Crates with no published version, or with only
                         pre-0.1.0 name-holding placeholders such as gyral 0.0.1,
                         are skipped.
"""
import json
import re
from pathlib import Path
import subprocess
import sys
import tomllib
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
# Publication order: each crate depends only on crates before it.
CRATES = ('gyral-renderer-domain', 'gyral-renderer', 'gyral-axum', 'gyral')
INDEX = 'https://index.crates.io'


def workspace_version():
    return tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']


def index_path(name):
    name = name.lower()
    if len(name) <= 2:
        return f'{len(name)}/{name}'
    if len(name) == 3:
        return f'3/{name[0]}/{name}'
    return f'{name[0:2]}/{name[2:4]}/{name}'


def published_versions(name):
    """Versions in the sparse index, including yanked ones; empty when the crate is absent."""
    request = urllib.request.Request(f'{INDEX}/{index_path(name)}', headers={'User-Agent': 'gyral-rs-release (https://github.com/gyraljs/gyral-rs)'})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            lines = response.read().decode().splitlines()
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return []
        raise
    return [json.loads(line)['vers'] for line in lines if line.strip()]


def pending():
    version = workspace_version()
    for name in CRATES:
        if version in published_versions(name):
            print(f'{name} {version} is already on crates.io; skipping', file=sys.stderr)
        else:
            print(name)


def release_key(version):
    """Numeric (major, minor, patch); pre-release and build suffixes are ignored."""
    return tuple(int(part) for part in re.match(r'(\d+)\.(\d+)\.(\d+)', version).groups())


def semver():
    checked = 0
    for name in CRATES:
        versions = published_versions(name)
        if not versions:
            print(f'{name}: no published baseline yet; semver check skipped')
            continue
        # A 0.0.x release reserved the name and has no API to keep compatible.
        if max(map(release_key, versions)) < (0, 1, 0):
            print(f'{name}: only placeholder {", ".join(versions)} published; semver check skipped')
            continue
        subprocess.run(['cargo', 'semver-checks', '--package', name, '--all-features'], cwd=ROOT, check=True)
        checked += 1
    print(f'Semver checks ran for {checked} of {len(CRATES)} crates.')


def notes():
    version = workspace_version()
    changelog = (ROOT / 'CHANGELOG.md').read_text()
    match = re.search(rf'^## \[{re.escape(version)}\] - (\S+)\n(.*?)(?=^## |\Z)', changelog, re.M | re.S)
    if not match or not match.group(2).strip():
        raise SystemExit(f'CHANGELOG.md has no section for {version}')
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', match.group(1)):
        raise SystemExit(f'Set the release date in the CHANGELOG.md heading for {version}')
    print(match.group(2).strip())


if __name__ == '__main__':
    commands = {'version': lambda: print(workspace_version()), 'notes': notes, 'pending': pending, 'semver': semver}
    if len(sys.argv) != 2 or sys.argv[1] not in commands:
        raise SystemExit(__doc__)
    commands[sys.argv[1]]()
