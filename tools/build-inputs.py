#!/usr/bin/env python3
"""Fingerprint reviewed compiler inputs without reading application data."""
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def fingerprint():
    names = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'Dockerfile', '.cargo/config.toml', 'package.json', 'pnpm-lock.yaml', 'tools/build-example.mjs', 'tools/v8-archive.json']
    paths = [ROOT / name for name in names]
    for directory in ['crates', 'examples']:
        paths.extend(path for path in (ROOT / directory).rglob('*') if path.is_file() and (path.suffix == '.rs' or path.name == 'Cargo.toml' or 'web' in path.relative_to(ROOT).parts))
    result = hashlib.sha256()
    for path in sorted(set(paths)):
        result.update(str(path.relative_to(ROOT)).encode() + b'\0')
        result.update(hashlib.sha256(path.read_bytes()).digest())
    return result.hexdigest()


if __name__ == '__main__':
    print(fingerprint())
