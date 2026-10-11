#!/usr/bin/env python3
"""Download the supported native archive and verify its reviewed checksum."""
import hashlib
import json
import os
from pathlib import Path
import platform
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parent.parent


def verify(path, pin):
    if path.stat().st_size != pin['bytes']:
        raise ValueError('V8 archive size mismatch')
    with path.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    if digest != pin['sha256']:
        raise ValueError('V8 archive SHA-256 mismatch')


def prepare():
    if platform.system() != 'Linux' or platform.machine() not in ('x86_64', 'AMD64'):
        raise ValueError('Only Linux x86_64 GNU builds have a reviewed native archive')
    pin = json.loads((ROOT / 'tools/v8-archive.json').read_text())
    destination = ROOT / '.local/v8' / pin['file']
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        verify(destination, pin)
    else:
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as output:
                temporary = Path(output.name)
                request = urllib.request.Request(pin['url'], headers={'User-Agent': 'gyral-rs-native-pin'})
                with urllib.request.urlopen(request, timeout=60) as source:
                    remaining = pin['bytes'] + 1
                    while remaining:
                        chunk = source.read(min(1024 * 1024, remaining))
                        if not chunk:
                            break
                        output.write(chunk)
                        remaining -= len(chunk)
            verify(temporary, pin)
            os.replace(temporary, destination)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
    print(f'Native V8 archive verified: {pin["version"]} {pin["target"]} SHA-256 {pin["sha256"]}')


if __name__ == '__main__':
    prepare()
