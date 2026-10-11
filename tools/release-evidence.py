#!/usr/bin/env python3
"""Assemble local, reviewable release artifacts; never publish or deploy."""
import gzip
import base64
import hashlib
import json
import re
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
TARGET = 'x86_64-unknown-linux-gnu'


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def clean_bom(bom):
    # Cargo uses absolute local paths for workspace refs. Replace every reference,
    # including dependency edges, before writing a shareable inventory.
    text = json.dumps(bom).replace(f'path+file://{ROOT}/', 'workspace:')
    result = json.loads(text)
    for component in [result['metadata']['component'], *result.get('components', [])]:
        if component.get('bom-ref', '').startswith('workspace:'):
            component['purl'] = f'pkg:cargo/{component["name"]}@{component["version"]}'
    return result


def npm_packages():
    """Retain pnpm's resolved paths/versions without assuming its store layout."""
    packages = {}
    def visit(dependencies):
        for name, dependency in sorted(dependencies.items()):
            key = (name, dependency['version'])
            if key not in packages:
                packages[key] = dependency
                visit(dependency.get('dependencies', {}))
    graph = json.loads(run('pnpm', 'list', '--prod', '--depth', 'Infinity', '--json'))[0]
    visit(graph.get('dependencies', {}))
    return packages


def npm_bom(packages=None):
    """Inventory the installed production graph; archive hashes come from pnpm."""
    packages = npm_packages() if packages is None else packages
    lock = (ROOT / 'pnpm-lock.yaml').read_text()
    components = {}
    for (name, version), dependency in packages.items():
        package = json.loads((Path(dependency['path']) / 'package.json').read_text())
        if package['name'] != name or package['version'] != version:
            raise ValueError('Installed npm package version differs from its graph')
        match = re.search(rf"  ['\"]?{re.escape(name)}@{re.escape(package['version'])}['\"]?:\n    resolution: \{{integrity: sha512-([A-Za-z0-9+/]+=*)\}}", lock)
        if not match:
            raise ValueError(f'No registry integrity for {name}')
        checksum = base64.b64decode(match[1], validate=True)
        if len(checksum) != 64:
            raise ValueError('Malformed registry SHA-512 integrity')
        reference = f'pkg:npm/{name.replace("@", "%40")}@{package["version"]}'
        components[(name, version)] = {
            'type': 'library', 'bom-ref': reference, 'name': name,
            'version': package['version'], 'licenses': [{'expression': package['license']}],
            'purl': reference, 'hashes': [{'alg': 'SHA-512', 'content': checksum.hex()}],
            'properties': [{'name': 'gyral:inventory:scope', 'value': 'installed npm production graph; type-only imports may be erased'}],
        }
    edges = []
    for key, dependency in sorted(packages.items()):
        edges.append({'ref': components[key]['bom-ref'], 'dependsOn': [components[(name, child['version'])]['bom-ref'] for name, child in sorted(dependency.get('dependencies', {}).items())]})
    return {'bomFormat': 'CycloneDX', 'specVersion': '1.5', 'version': 1, 'components': list(components.values()), 'dependencies': edges}


def source_files():
    roots = ['.agents', '.cargo', '.config', '.github', '.sqlx', 'crates', 'docs', 'examples', 'tools']
    names = ['.dockerignore', '.gitignore', '.node-version', 'AGENTS.md', 'ARCHITECTURE.md', 'CONTRIBUTING.md', 'SECURITY.md', 'CODE_OF_CONDUCT.md', 'Cargo.toml', 'Cargo.lock', 'Dockerfile', 'Dockerfile.client', 'LICENSE', 'Makefile', 'README.md', 'THIRD_PARTY_NOTICES.md', 'clippy.toml', 'deny.toml', 'package.json', 'pnpm-lock.yaml', 'rust-toolchain.toml', 'rustfmt.toml', 'skills.lock.json']
    files = [ROOT / name for name in names]
    for directory in roots:
        for path in (ROOT / directory).rglob('*'):
            if path.is_file() and 'dist' not in path.relative_to(ROOT).parts and '__pycache__' not in path.parts and not path.name.endswith('.cdx.json'):
                if path.is_symlink():
                    raise ValueError('Source artifacts must not follow symlinks')
                files.append(path)
    return sorted(files)


def archive(output, paths, prefix, base):
    # Fixed metadata makes a repeated archive of unchanged inputs reproducible.
    with output.open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0, filename='') as compressed, tarfile.open(fileobj=compressed, mode='w') as contents:
        for path in sorted(paths):
            info = contents.gettarinfo(str(path), arcname=f'{prefix}/{path.relative_to(base)}')
            info.uid = info.gid = info.mtime = 0
            info.uname = info.gname = ''
            info.mode = 0o755 if path.name == 'gyral-axum-example' else 0o644
            with path.open('rb') as source:
                contents.addfile(info, source)


def main():
    if run('cargo', 'cyclonedx', '--version') != 'cargo-cyclonedx-cyclonedx 0.5.9':
        raise ValueError('Install the pinned cargo-cyclonedx 0.5.9')
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    output = ROOT / '.local/release' / version
    output.mkdir(parents=True, exist_ok=True)
    pin = json.loads((ROOT / 'tools/v8-archive.json').read_text())
    run('python3', 'tools/prepare-v8.py')
    lock_digest = digest(ROOT / 'Cargo.lock')
    run('cargo', 'cyclonedx', '--format', 'json', '--all', '--spec-version', '1.5', '--target', TARGET, '--describe', 'binaries')
    assert digest(ROOT / 'Cargo.lock') == lock_digest, 'SBOM generation changed Cargo.lock'
    bom_path = ROOT / 'examples/axum-app/gyral-axum-example_bin.cdx.json'
    bom = clean_bom(json.loads(bom_path.read_text()))
    native_ref = 'native-v8-archive'
    bom['components'].append({
        'type': 'library', 'bom-ref': native_ref, 'name': 'rusty-v8-native-archive', 'version': pin['version'],
        'licenses': [{'expression': 'BSD-3-Clause'}],
        'hashes': [{'alg': 'SHA-256', 'content': pin['sha256']}],
        'externalReferences': [{'type': 'distribution', 'url': pin['url']}],
        'properties': [{'name': 'gyral:target', 'value': TARGET}, {'name': 'gyral:archive:provenance', 'value': pin['provenance']}],
    })
    v8 = next(component for component in bom['components'] if component['name'] == 'v8')
    next(edge for edge in bom['dependencies'] if edge['ref'] == v8['bom-ref'])['dependsOn'].append(native_ref)
    bom['dependencies'].append({'ref': native_ref, 'dependsOn': []})
    javascript_packages = npm_packages()
    npm = npm_bom(javascript_packages)
    bom['components'].extend(npm['components'])
    bom['dependencies'].extend(npm['dependencies'])
    for component in npm['components']:
        if component['name'] in ('@gyral/core', '@gyral/ssr'):
            next(edge for edge in bom['dependencies'] if edge['ref'] == bom['metadata']['component']['bom-ref'])['dependsOn'].append(component['bom-ref'])
    for filename, document in [('ssr.cdx.json', bom), ('npm-production.cdx.json', npm)]:
        (output / filename).write_text(json.dumps(document, indent=2) + '\n')
    shutil.copyfile(ROOT / 'tools/v8-archive.json', output / 'v8-archive.json')
    shutil.copyfile(ROOT / 'Cargo.lock', output / 'Cargo.lock')
    shutil.copyfile(ROOT / 'pnpm-lock.yaml', output / 'pnpm-lock.yaml')
    image = run('docker', 'image', 'inspect', '--format', '{{.Id}}', 'gyral-rs:example')
    input_hash = run('python3', 'tools/build-inputs.py')
    assert run('docker', 'image', 'inspect', '--format', '{{index .Config.Labels \"org.gyral-rs.input-sha256\"}}', image) == input_hash, 'Docker compiler inputs differ; rebuild the image'
    with tempfile.TemporaryDirectory(prefix='gyral-release-') as directory:
        stage = Path(directory)
        container = run('docker', 'create', image)
        try:
            run('docker', 'cp', f'{container}:/usr/local/bin/gyral-axum-example', str(stage / 'gyral-axum-example'))
            run('docker', 'cp', f'{container}:/app/dist', str(stage / 'dist'))
        finally:
            run('docker', 'rm', container)
        # Reject a stale image rather than pairing a new lockfile with old bytes.
        assert (stage / 'dist/manifest.json').read_bytes() == (ROOT / 'examples/axum-app/dist/manifest.json').read_bytes(), 'Docker assets differ; rebuild the image'
        metadata = json.loads(run('cargo', 'metadata', '--locked', '--format-version', '1'))
        inventory = {(component['name'], component['version']) for component in bom['components']}
        for package in metadata['packages']:
            if (package['name'], package['version']) not in inventory and package['name'] != 'webpki-root-certs':
                continue
            package_root = Path(package['manifest_path']).parent
            texts = [path for pattern in ['LICENSE*', 'LICENCE*', 'COPYING*', 'NOTICE*'] for path in package_root.glob(pattern) if path.is_file()]
            for text in texts:
                destination = stage / 'licenses' / f"{package['name']}-{package['version']}" / text.name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(text, destination)
        certificate_license = stage / 'licenses/webpki-root-certs-1.0.9/LICENSE'
        if not any(certificate_license.parent.glob('LICENSE*')):
            raise ValueError('The TLS certificate-data agreement must ship with the binary')
        for name in ['LICENSE', 'THIRD_PARTY_NOTICES.md']:
            shutil.copyfile(ROOT / name, stage / name)
        for component in npm['components']:
            package_root = Path(javascript_packages[(component['name'], component['version'])]['path'])
            for pattern in ['LICENSE*', 'LICENCE*', 'NOTICE*']:
                for text in package_root.glob(pattern):
                    if text.is_file():
                        destination = stage / 'licenses' / f'{component["name"].replace("/", "-")}-{component["version"]}' / text.name
                        destination.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copyfile(text, destination)
        (stage / 'RUN.md').write_text('Run on Linux x86_64 with glibc 2.36+, libstdc++ and CA certificates.\n\nGYRAL_EXAMPLE_BIND=127.0.0.1:3037 GYRAL_EXAMPLE_DIST=./dist ./gyral-axum-example\n\nUse /livez for liveness and /readyz for readiness. This synthetic trusted-code example provides no application authentication.\n')
        binary_name = f'gyral-rs-{version}-{TARGET}.tar.gz'
        archive(output / binary_name, [path for path in stage.rglob('*') if path.is_file()], f'gyral-rs-{version}', stage)
    source_name = f'gyral-rs-{version}-source.tar.gz'
    archive(output / source_name, source_files(), f'gyral-rs-{version}', ROOT)
    descriptor = {
        'version': version, 'target': TARGET, 'msrv': '1.99.0', 'distribution': 'local review artifacts; no publication',
        'reference_image': image, 'build_input_sha256': input_hash, 'rustc': run('rustc', '--version'), 'native_archive_sha256': pin['sha256'],
        'cargo_lock_sha256': digest(ROOT / 'Cargo.lock'), 'pnpm_lock_sha256': digest(ROOT / 'pnpm-lock.yaml'),
        'bundle_manifest_sha256': digest(ROOT / 'examples/axum-app/dist/manifest.json'),
        'scope': 'Rust dependency/build inputs, native V8 archive, and Gyral inputs. OS packages and JavaScript build/test tools require separate container/toolchain inventories.',
        'artifacts': [binary_name, source_name, 'ssr.cdx.json', 'npm-production.cdx.json', 'v8-archive.json', 'Cargo.lock', 'pnpm-lock.yaml'],
    }
    (output / 'release.json').write_text(json.dumps(descriptor, indent=2) + '\n')
    names = [*descriptor['artifacts'], 'release.json']
    (output / 'SHA256SUMS').write_text(''.join(f'{digest(output / name)}  {name}\n' for name in names))
    print(f'Local release evidence: {output.relative_to(ROOT)} ({len(bom["components"])} SSR components, {len(npm["components"])} npm production inputs). No artifacts published.')


if __name__ == '__main__':
    main()
