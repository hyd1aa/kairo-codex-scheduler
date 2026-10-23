#!/usr/bin/env python3
"""Sign a candidate locally/inside a protected CI environment; never publish."""
import hashlib,json,os,pathlib,subprocess,sys
repo=pathlib.Path(__file__).resolve().parents[1]
out=pathlib.Path(sys.argv[1]).resolve();key=pathlib.Path(sys.argv[2]).resolve()
assert not key.is_relative_to(repo),'Private signing key must be outside checkout'
assert key.is_file() and key.stat().st_mode & 0o077 == 0,'Private key must be mode0600'
version='0.2.1'
expected=(repo/'packaging/RELEASE-PUBLIC-KEY.pem').read_bytes()
actual=subprocess.check_output(['openssl','pkey','-in',str(key),'-pubout'],stderr=subprocess.DEVNULL)
assert actual==expected,'Signing identity does not match pinned public key'
files=[f'kairo-{version}.tar.gz',f'kairo-{version}-x86_64-unknown-linux-gnu',f'kairo-{version}-aarch64-unknown-linux-gnu','one-click.sh','bootstrap-signed.sh','signed-update.sh','release-auth.sh','sbom.spdx.json','release-metadata.json']
for name in ('one-click.sh','bootstrap-signed.sh','signed-update.sh','release-auth.sh'):(out/name).write_bytes((repo/'packaging'/name).read_bytes())
manifest=out/f'kairo-{version}.sha256'
for name in files:assert (out/name).is_file(),name
manifest.write_text(''.join(hashlib.sha256((out/name).read_bytes()).hexdigest()+'  '+name+'\n' for name in sorted(files)))
subprocess.run(['openssl','pkeyutl','-sign','-inkey',str(key),'-rawin','-in',str(manifest),'-out',str(manifest)+'.sig'],check=True,stderr=subprocess.DEVNULL)
for name in files:subprocess.run(['sh',str(repo/'packaging/release-auth.sh'),version,str(out),name],check=True,stdout=subprocess.DEVNULL)
(out/'RELEASE-PUBLIC-KEY.pem').write_bytes(expected)
print('Pinned Ed25519 signature and all nine release assets verified; no publication performed')
