#!/usr/bin/env python3
"""Regression checks for archive verification and private-data exclusion."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'tools' / f'{name}.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


native = load('prepare-v8')
release = load('release-evidence')
crates = load('release')


class BuildInputs(unittest.TestCase):
    def test_corrupt_or_truncated_archives_fail_verification(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'native.gz'
            path.write_bytes(b'archive')
            pin = {'bytes': 7, 'sha256': hashlib.sha256(b'archive').hexdigest()}
            native.verify(path, pin)
            path.write_bytes(b'Archive')
            with self.assertRaisesRegex(ValueError, 'SHA-256'):
                native.verify(path, pin)
            path.write_bytes(b'archive EXTRA')
            with self.assertRaisesRegex(ValueError, 'size'):
                native.verify(path, pin)

    def test_source_distribution_excludes_local_data_and_generated_files(self):
        paths = [path.relative_to(ROOT) for path in release.source_files()]
        self.assertIn(Path('Cargo.lock'), paths)
        self.assertIn(Path('pnpm-workspace.yaml'), paths)
        self.assertIn(Path('tools/v8-archive.json'), paths)
        # tools/check.py runs inside the extracted archive and requires these documents.
        for document in ('AGENTS.md', 'README.md', 'CHANGELOG.md', 'ARCHITECTURE.md', 'THIRD_PARTY_NOTICES.md', 'CONTRIBUTING.md', 'SECURITY.md', 'CODE_OF_CONDUCT.md'):
            self.assertIn(Path(document), paths)
        self.assertFalse(any(set(path.parts) & {'.beads', '.git', '.local', 'target', 'node_modules', 'dist', '__pycache__'} for path in paths))
        self.assertFalse(any(path.name.endswith(('.sqlite', '.cdx.json', '.pem', '.key')) for path in paths))
        bom = release.npm_bom()
        self.assertEqual({item['name'] for item in bom['components']}, {'@gyral/core', '@gyral/ssr', '@standard-schema/spec'})
        core = next(item for item in bom['components'] if item['name'] == '@gyral/core')
        self.assertEqual(core['version'], '0.3.9')
        self.assertEqual(core['hashes'][0]['alg'], 'SHA-512')
        self.assertEqual(next(edge for edge in bom['dependencies'] if edge['ref'] == core['bom-ref'])['dependsOn'], ['pkg:npm/%40standard-schema/spec@1.1.0'])

    def test_cargo_workspace_refs_are_redacted_consistently(self):
        reference = f'path+file://{ROOT}/crates/domain#gyral-renderer-domain@0.1.0'
        component = {'bom-ref': reference, 'name': 'gyral-renderer-domain', 'version': '0.1.0'}
        fixture = {'metadata': {'component': component}, 'components': [], 'dependencies': [{'ref': reference, 'dependsOn': [reference]}]}
        bom = release.clean_bom(fixture)
        self.assertNotIn(str(ROOT), json.dumps(bom))
        self.assertEqual(bom['dependencies'][0]['ref'], bom['metadata']['component']['bom-ref'])
        self.assertEqual(bom['dependencies'][0]['dependsOn'], [bom['metadata']['component']['bom-ref']])

    def test_archives_repeat_bytes_and_preserve_only_intended_files(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            source = base / 'input.rs'
            source.write_text('synthetic source')
            archives = [base / 'first.tar.gz', base / 'second.tar.gz']
            for output in archives:
                release.archive(output, [source], 'gyral-rs-test', base)
            self.assertEqual(archives[0].read_bytes(), archives[1].read_bytes())
            with tarfile.open(archives[0]) as archive:
                self.assertEqual(archive.getnames(), ['gyral-rs-test/input.rs'])
                self.assertEqual(archive.extractfile(archive.getmembers()[0]).read(), b'synthetic source')
                self.assertEqual(archive.getmembers()[0].uid, 0)


class CratesRelease(unittest.TestCase):
    def test_semver_checks_skip_missing_and_placeholder_baselines(self):
        published = {'gyral-renderer-domain': [], 'gyral-renderer': ['0.0.1'], 'gyral-axum': ['0.1.0'], 'gyral': ['0.0.1', '0.0.2-rc.1']}
        calls = []
        original = (crates.published_versions, crates.subprocess.run)
        crates.published_versions = published.__getitem__
        crates.subprocess.run = lambda command, **options: calls.append(command)
        try:
            crates.semver()
        finally:
            crates.published_versions, crates.subprocess.run = original
        self.assertEqual(calls, [['cargo', 'semver-checks', '--package', 'gyral-axum', '--all-features']])


if __name__ == '__main__':
    unittest.main()
