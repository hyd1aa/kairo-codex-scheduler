#!/usr/bin/env python3
"""Read-only final asset gate. Never executes a binary, installs, tags or publishes."""
import argparse
import hashlib
import json
import pathlib
import re
import subprocess

REPO = pathlib.Path(__file__).resolve().parents[1]
VERSION = '0.2.1'
ASSETS = sorted([
    f'kairo-{VERSION}.tar.gz',
    f'kairo-{VERSION}-x86_64-unknown-linux-gnu',
    f'kairo-{VERSION}-aarch64-unknown-linux-gnu',
    'one-click.sh', 'bootstrap-signed.sh', 'signed-update.sh',
    'release-auth.sh', 'sbom.spdx.json', 'release-metadata.json',
])


def verify(directory, source, manifest_sha):
    if not re.fullmatch(r'[0-9a-f]{40}', source):
        raise ValueError('Expected full source commit')
    if not re.fullmatch(r'[0-9a-f]{64}', manifest_sha):
        raise ValueError('Expected independently reviewed manifest SHA256')
    manifest_name = f'kairo-{VERSION}.sha256'
    required = ASSETS + [manifest_name, manifest_name + '.sig', 'RELEASE-PUBLIC-KEY.pem']
    for name in required:
        path = directory / name
        if path.is_symlink() or not path.is_file():
            raise ValueError('Missing or linked release file: ' + name)
    manifest = (directory / manifest_name).read_bytes()
    if len(manifest) > 16384 or hashlib.sha256(manifest).hexdigest() != manifest_sha:
        raise ValueError('Manifest SHA256 mismatch')
    rows = manifest.decode('ascii').splitlines()
    parsed = [re.fullmatch(r'([0-9a-f]{64})  ([A-Za-z0-9_.-]+)', row) for row in rows]
    if not all(parsed) or [row[2] for row in parsed] != ASSETS:
        raise ValueError('Manifest must bind exactly nine canonical assets')
    public = (REPO / 'packaging/RELEASE-PUBLIC-KEY.pem').read_bytes()
    if (directory / 'RELEASE-PUBLIC-KEY.pem').read_bytes() != public:
        raise ValueError('Candidate public key differs from locally pinned key')
    for name in ASSETS:
        subprocess.run(['sh', str(REPO / 'packaging/release-auth.sh'), VERSION,
                        str(directory), name], check=True, stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL)
    metadata = json.loads((directory / 'release-metadata.json').read_text())
    if metadata.get('source_commit') != source or metadata.get('version') != VERSION:
        raise ValueError('Signed metadata source/version mismatch')
    return {'version': VERSION, 'source_commit': source, 'manifest_sha256': manifest_sha,
            'bootstrap_sha256': hashlib.sha256((directory / 'bootstrap-signed.sh').read_bytes()).hexdigest(),
            'public_pem_sha256': hashlib.sha256(public).hexdigest(),
            'release_assets': required, 'publisher_identity_verified': False,
            'public_endpoint_tested': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=pathlib.Path)
    parser.add_argument('--source', required=True)
    parser.add_argument('--manifest-sha256', required=True)
    args = parser.parse_args()
    try:
        print(json.dumps(verify(args.directory.resolve(), args.source, args.manifest_sha256), indent=2))
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit('Release refused: ' + str(error))
